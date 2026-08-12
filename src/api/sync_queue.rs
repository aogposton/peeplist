// Offline-first sync for the Synced vault — the persisted queue half (see
// synced_mirror.rs for the read-side cache). SupabaseStorage's write
// methods (storage.rs) push a QueuedOp here and return immediately instead
// of awaiting the network; the background flush loop in layouts/navbar.rs
// drains this and replays each op against the real API once it can. One
// JSON array under a single localStorage key — the same
// web_sys::window().local_storage() idiom used everywhere else in this app
// (local.rs, main.rs's startup effect, navbar.rs's token persistence), no
// new storage primitive.
//
// Encode/decode (and drop_front) are kept pure (no web_sys) so they're
// unit-testable the same way vault_format.rs is; only push/peek/
// remove_front/clear touch the browser.

use crate::types::*;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use web_sys::Storage;

const QUEUE_KEY: &str = "peeplist_synced_cache:queue";

// Bumped by every `push` below — layouts/navbar.rs watches this (a plain
// counter, not the queue contents themselves) to trigger an immediate flush
// attempt right after something's queued, instead of waiting for the next
// periodic tick or 'online' event. Global rather than threaded through
// AppState because `push` is called from storage.rs's free functions,
// which have no `use_context` access (they're not components) — a
// GlobalSignal is readable/writable from anywhere once the app's running,
// no plumbing required. The periodic/online-listener triggers stay exactly
// as they were: this is a third, faster trigger for the common case
// (genuinely online), not a replacement for the offline fallback.
pub static FLUSH_REQUESTED: GlobalSignal<u64> = GlobalSignal::new(|| 0);

// temp_id -> real server id, accumulated forever (until `clear()`, i.e.
// logout) across every flush pass — not just within a single one. The flush
// loop's own per-pass `id_map` only remembers a create's remapping for the
// rest of *that* pass; anything referencing the same temp_id in a *later*
// pass (a field edit staged before the create had synced, but only queued
// or retried after it — e.g. via a cached snapshot elsewhere in the app
// that doesn't track the live moments/entities Signal, like
// components/moment.rs's OnTheFlyCmp) found nothing to remap it with and
// kept sending the server a request built around an id that was never
// real. Confirmed live: exactly this, retrying forever every tick,
// hammering the API with a request PostgREST can only ever reject.
// Seeding each pass's id_map from this closes that gap regardless of which
// disconnected snapshot leaks a stale id, current or future.
pub static RESOLVED_IDS: GlobalSignal<HashMap<String, String>> = GlobalSignal::new(HashMap::new);

// Create ops carry a client-minted `temp_id` — the record is shown in the
// UI (and written into the mirror) under this id the instant it's created,
// long before the server assigns a real one. The flush loop remaps
// `temp_id` to the server's real id in every op still queued behind this
// one (and in the live Signal/mirror) once the create actually lands — see
// navbar.rs's flush loop.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum QueuedOp {
    CreateMoment { temp_id: String, new: NewMomentType },
    CreateEntity { temp_id: String, new: NewEntityType },
    CreateReaction { temp_id: String, new: NewReactionType },
    // `staged_updated_at` is the mirror's last known server-confirmed
    // `updated_at` for this record at the moment this edit was staged —
    // NOT bumped by any of this device's own not-yet-flushed local edits
    // (that would compare a client clock against the server's, which can
    // drift). The flush loop (layouts/navbar.rs) fetches the record's
    // current server-side updated_at and compares: if it's newer than
    // this, someone else changed the row since we last synced from it, and
    // the server wins instead of being silently overwritten.
    UpdateMomentField { id: String, field: String, value: Value, staged_updated_at: String },
    UpdateEntityField { id: String, field: String, value: Value, staged_updated_at: String },
    DeleteMoment(MomentType),
    DeleteEntity(String),
    DeleteReaction(ReactionType),
    RestoreMoment(String),
}

fn encode(queue: &[QueuedOp]) -> String {
    serde_json::to_string(queue).unwrap_or_else(|_| "[]".to_string())
}

// Kept pure (no web_sys) — see the module doc comment — so this stays
// unit-testable with plain garbage strings. `load` below (which does touch
// the browser) is where a parse failure actually gets logged; this just
// reports it rather than swallowing it via `unwrap_or_default`.
fn decode(raw: &str) -> Result<Vec<QueuedOp>, serde_json::Error> {
    serde_json::from_str(raw)
}

fn local_storage() -> Option<Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

// A parse failure here used to be silent and total: one malformed element
// anywhere in the array fails the whole array, `unwrap_or_default` turned
// that into an empty queue, and every op in it — not just the bad one —
// was gone from `peek`'s point of view forever, with nothing left in
// storage to show it ever existed (see NewMomentType::entity_id's doc
// comment for the actual bug that produced exactly this). Logging here
// doesn't fix a bad payload, but it turns "every create silently stops
// working, no error anywhere" into something that at least shows up in the
// console the moment it starts happening.
fn load(storage: &Storage) -> Vec<QueuedOp> {
    let Some(raw) = storage.get_item(QUEUE_KEY).ok().flatten() else { return Vec::new(); };
    match decode(&raw) {
        Ok(queue) => queue,
        Err(e) => {
            web_sys::console::warn_1(&format!("sync_queue: failed to decode persisted queue, treating as empty ({e}): {raw}").into());
            Vec::new()
        }
    }
}

fn save(storage: &Storage, queue: &[QueuedOp]) {
    let _ = storage.set_item(QUEUE_KEY, &encode(queue));
}

pub fn push(op: QueuedOp) {
    let Some(storage) = local_storage() else { return };
    let mut queue = load(&storage);
    queue.push(op);
    save(&storage, &queue);
    *FLUSH_REQUESTED.write() += 1;
}

// Reads the queue without removing anything — the flush loop processes
// each op it sees here one at a time, only calling `remove_front` once an
// op has actually reached a definitive outcome (see remove_front's doc
// comment for why this split matters). Never mutates storage itself, so
// calling this costs nothing to "undo."
pub fn peek() -> Vec<QueuedOp> {
    let Some(storage) = local_storage() else { return Vec::new() };
    load(&storage)
}

// Removes exactly the first `n` entries from the front of the persisted
// queue, leaving everything after them untouched — including anything
// pushed here *during* the current flush pass (new local edits go to the
// back via `push`, this only ever removes from the front, so the two never
// collide).
//
// This replaced an eager "drain the whole queue up front, replay each op,
// requeue whatever failed at the very end" design. That was genuinely
// unsafe: `drain` emptied the persisted queue immediately, before a single
// op had actually been confirmed sent. If the flush pass was interrupted
// anywhere between the drain and the final requeue call — a hung request
// that never resolves, a tab closing mid-flush, anything — every op that
// hadn't explicitly finished replaying was gone for good, with nothing
// left recording that it had ever been queued. Confirmed live: creating a
// moment while offline, watching it get written to the queue, and watching
// the queue get wiped by the next flush tick with the moment never
// actually reaching the server. Calling `remove_front(1)` immediately
// after each op reaches a real outcome (success, or a definitive server
// rejection) means the persisted queue is always an accurate picture of
// "what's actually still left to do" — an interruption at any point loses
// nothing beyond whatever was already, legitimately, done.
pub fn remove_front(n: usize) {
    let Some(storage) = local_storage() else { return };
    let queue = drop_front(load(&storage), n);
    save(&storage, &queue);
}

fn drop_front(mut queue: Vec<QueuedOp>, n: usize) -> Vec<QueuedOp> {
    if n >= queue.len() {
        Vec::new()
    } else {
        queue.drain(0..n);
        queue
    }
}

pub fn clear() {
    if let Some(storage) = local_storage() {
        let _ = storage.remove_item(QUEUE_KEY);
    }
    RESOLVED_IDS.write().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_op(id: &str) -> QueuedOp {
        QueuedOp::UpdateMomentField {
            id: id.to_string(),
            field: "title".to_string(),
            value: Value::String("hi".to_string()),
            staged_updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn encode_decode_round_trips_every_variant() {
        let ops = vec![
            QueuedOp::CreateMoment {
                temp_id: "t1".to_string(),
                new: NewMomentType {
                    title: "Call Mom".to_string(),
                    description: None,
                    gravity: Some(1),
                    entity_id: "e1".to_string(),
                    moment_type_id: 1,
                    deleted_at: None,
                },
            },
            QueuedOp::CreateEntity {
                temp_id: "t2".to_string(),
                new: NewEntityType {
                    name: "Alex".to_string(),
                    entity_type_id: None,
                    parent_entity_id: None,
                    user_id: None,
                    archived_at: None,
                    metadata: None,
                },
            },
            QueuedOp::CreateReaction {
                temp_id: "t3".to_string(),
                new: NewReactionType { description: "nice".to_string(), moment_id: "m1".to_string(), value: 1 },
            },
            sample_op("m1"),
            QueuedOp::UpdateEntityField { id: "e1".to_string(), field: "name".to_string(), value: Value::String("Alexandra".to_string()), staged_updated_at: "2026-01-01T00:00:00Z".to_string() },
            QueuedOp::DeleteEntity("e2".to_string()),
            QueuedOp::RestoreMoment("m2".to_string()),
        ];
        let raw = encode(&ops);
        let round_tripped = decode(&raw).unwrap();
        assert_eq!(ops, round_tripped);
    }

    #[test]
    fn decode_of_garbage_is_an_error_not_a_panic() {
        assert!(decode("not json").is_err());
        assert!(decode("").is_err());
    }

    #[test]
    fn decode_of_empty_array_is_empty_queue() {
        assert_eq!(decode("[]").unwrap(), Vec::new());
    }

    #[test]
    fn drop_front_removes_exactly_the_leading_n_entries() {
        let queue = vec![sample_op("a"), sample_op("b"), sample_op("c")];
        let remaining = drop_front(queue, 2);
        assert_eq!(remaining, vec![sample_op("c")]);
    }

    #[test]
    fn drop_front_of_zero_leaves_everything() {
        let queue = vec![sample_op("a"), sample_op("b")];
        assert_eq!(drop_front(queue.clone(), 0), queue);
    }

    #[test]
    fn drop_front_of_everything_or_more_empties_the_queue() {
        let queue = vec![sample_op("a"), sample_op("b")];
        assert_eq!(drop_front(queue.clone(), 2), Vec::new());
        assert_eq!(drop_front(queue, 5), Vec::new());
    }
}
