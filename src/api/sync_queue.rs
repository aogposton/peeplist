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
use serde::{Deserialize, Serialize};
use serde_json::Value;
use web_sys::Storage;

const QUEUE_KEY: &str = "peeplist_synced_cache:queue";

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

fn decode(raw: &str) -> Vec<QueuedOp> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn local_storage() -> Option<Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

fn load(storage: &Storage) -> Vec<QueuedOp> {
    storage.get_item(QUEUE_KEY).ok().flatten().map(|raw| decode(&raw)).unwrap_or_default()
}

fn save(storage: &Storage, queue: &[QueuedOp]) {
    let _ = storage.set_item(QUEUE_KEY, &encode(queue));
}

pub fn push(op: QueuedOp) {
    let Some(storage) = local_storage() else { return };
    let mut queue = load(&storage);
    queue.push(op);
    save(&storage, &queue);
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
        let round_tripped = decode(&raw);
        assert_eq!(ops, round_tripped);
    }

    #[test]
    fn decode_of_garbage_is_an_empty_queue_not_a_panic() {
        assert_eq!(decode("not json"), Vec::new());
        assert_eq!(decode(""), Vec::new());
    }

    #[test]
    fn decode_of_empty_array_is_empty_queue() {
        assert_eq!(decode("[]"), Vec::new());
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
