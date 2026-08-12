use crate::Route;
use crate::theme::*;
use dioxus::prelude::*;
use crate::AppState;
use crate::ui::*;
use crate::View;
use crate::View::*;
use crate::ABView;
// use dioxus_sdk_window::size::{get_window_size, use_window_size};
// use dioxus_sdk::utils::window::use_window_size;

use crate::components::{
    MomentCmp,
    MomentListCmp,
    MomentInputCmp,
    FullScreenEditorModalCmp,
    OnTheFlyCmp,
    ab_task_cmp,
    ab_story_cmp,
    ab_stats_cmp,
    ab_info_cmp,
    ab_momentos_cmp,
    views_list_cmp,
    entity_list_cmp,
    tag_list_cmp,
    project_list_cmp,
    NotesSectionCmp,
};

use crate::api::{
    get_current_user,
    refresh_access_token,
    VaultKind,
};

use crate::types::{EntityType, MomentType, NewMomentType};
use crate::api::sync_queue::{self, QueuedOp};
use crate::api::synced_mirror;
use web_sys::window;
use gloo_timers::future::TimeoutFuture;
use lumen_blocks::components::avatar::{Avatar, AvatarFallback};
use lumen_blocks::components::dropdown::{Dropdown, DropdownContent, DropdownItem, DropdownTrigger, DropdownSeparator};
use std::collections::{HashMap, HashSet};

// Refresh the access token this long before it would otherwise expire via
// inactivity/backend expiry, so a live session never silently dies underneath
// the user. Supabase's default JWT lifetime is 1 hour; 50 minutes leaves margin.
const TOKEN_REFRESH_INTERVAL_MS: u32 = 50 * 60 * 1000;

// Offline-first sync for the Synced vault (see api::sync_queue/
// synced_mirror). Much shorter than the token-refresh interval above —
// this is what makes a reconnect feel snappy rather than waiting up to an
// hour for queued edits to actually reach the server.
const SYNC_FLUSH_INTERVAL_MS: u32 = 60 * 1000;

// Below this width, the docked desktop sidebar (see Navbar's second
// sidebar block, distinct from the mobile drawer `Sidebar` component)
// auto-folds to give the content column its space back.
const SIDEBAR_FOLD_WIDTH: f64 = 900.0;

const ONLINE_SCRIPT: &str = r#"
    window.addEventListener('online', () => dioxus.send(true));
"#;

// Supabase rotates the refresh token on every use — the token that was
// just spent stops working the moment a new one comes back. With the
// 50-minute proactive loop below AND the 60-second sync flush loop
// (refresh_before_flush) each independently calling refresh_access_token
// on their own schedule, a long enough session guarantees their ticks
// eventually land close together: both read the same not-yet-rotated
// refresh_token from localStorage, one reaches Supabase first and rotates
// it, and the other's now-stale token gets rejected — which, depending on
// Supabase's reuse-detection settings, can invalidate the whole session.
// That's a real, live way for sync to silently and permanently stop
// working (every queued write then fails against a dead session forever,
// looking exactly like "nothing ever synced"), not just a wasted API call.
//
// This one shared clock is how every refresh call site (the mount-time
// check, the 50-minute loop, and the flush loop) coordinates: whichever of
// them actually refreshes stamps this, and none of them will refresh again
// until it's stale — so there's only ever one active "refresh clock" for
// the whole app, not three independent ones that can race each other.
const MIN_REFRESH_INTERVAL_SECS: i64 = 5 * 60;
pub(crate) const LAST_REFRESHED_AT_KEY: &str = "auth_last_refreshed_at";

fn is_refresh_due(last_refreshed_at: Option<i64>, now: i64) -> bool {
    match last_refreshed_at {
        Some(last) => now - last > MIN_REFRESH_INTERVAL_SECS,
        None => true,
    }
}

fn should_refresh_now(storage: &web_sys::Storage) -> bool {
    let last: Option<i64> = storage.get_item(LAST_REFRESHED_AT_KEY).ok().flatten().and_then(|s| s.parse().ok());
    is_refresh_due(last, chrono::Utc::now().timestamp())
}

fn mark_refreshed(storage: &web_sys::Storage) {
    storage.set(LAST_REFRESHED_AT_KEY, &chrono::Utc::now().timestamp().to_string()).ok();
}

// A create op's `temp_id` (client-minted, shown in the UI the instant it's
// created) only resolves to the server's real id once its own create
// actually replays successfully — everything else queued behind it that
// references that id (a field edit, a delete, a reaction) has to get the
// real id substituted in before it can be sent, or the server has no idea
// what row it's talking about. `id_map` accumulates temp_id -> real_id as
// each create in this flush pass resolves; these two helpers apply it.
fn remap_id(id: &str, id_map: &HashMap<String, String>) -> String {
    id_map.get(id).cloned().unwrap_or_else(|| id.to_string())
}

// Recurses into arrays/objects, not just a bare top-level string — the
// "metadata" field's queued value is a whole MomentMetadata object, and a
// temp id needing remap can be buried inside it (additional_entity_ids,
// depends_on), not just at the top level. A flat-only version of this
// (pre-2026-08-07) meant a moment co-created with a brand-new second entity
// in the same batch — e.g. a fresh @mention added inline via "+ Add..." —
// flushed to Supabase with metadata.additional_entity_ids still pointing at
// the client-only temp uuid, a reference that could never resolve to a real
// row server-side: the entity would sync fine on its own, just permanently
// disconnected from the moment that was supposed to reference it.
fn remap_value(value: serde_json::Value, id_map: &HashMap<String, String>) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => match id_map.get(&s) {
            Some(real) => serde_json::Value::String(real.clone()),
            None => serde_json::Value::String(s),
        },
        serde_json::Value::Array(arr) => serde_json::Value::Array(
            arr.into_iter().map(|v| remap_value(v, id_map)).collect()
        ),
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.into_iter().map(|(k, v)| (k, remap_value(v, id_map))).collect()
        ),
        other => other,
    }
}

// Real LWW: is the server's current updated_at strictly newer than the
// baseline this edit was staged against? Parses both as real timestamps
// rather than comparing the raw strings — Postgres's timestamptz
// serialization is consistent enough that string comparison would usually
// agree, but "usually" isn't good enough for a check whose only job is
// deciding whose edit survives. Fails open (not stale) if either side is
// empty or unparseable — a record with no known baseline, or an
// unrecognized timestamp shape, shouldn't block an edit from applying;
// that's the same call this app already makes elsewhere for anything
// date-related that might be malformed or missing (see momento.rs's
// is_revealed/urgency.rs's parse_moment_datetime).
fn is_stale(staged_updated_at: &str, server_updated_at: &str) -> bool {
    if staged_updated_at.is_empty() || server_updated_at.is_empty() {
        return false;
    }
    match (
        chrono::DateTime::parse_from_rfc3339(staged_updated_at),
        chrono::DateTime::parse_from_rfc3339(server_updated_at),
    ) {
        (Ok(staged), Ok(server)) => server > staged,
        _ => false,
    }
}

// Tries to keep the access token fresh before replaying anything queued —
// otherwise a long offline stretch means every queued op fails on an
// expired token, which looks like a completely different bug. Reuses the
// exact refresh flow the 50-minute loop above already does; falls back to
// whatever's currently cached (refresh itself needs network too, so this
// can fail while still offline — that's fine, the flush attempt below will
// just fail the same way and everything stays queued for the next tick).
//
// Gated by should_refresh_now/mark_refreshed (see their doc comment above)
// — this runs every 60 seconds, far more often than a token actually needs
// refreshing, so it only actually calls refresh_access_token when nothing
// else (this same function on an earlier tick, the 50-minute loop, or the
// mount-time check) has refreshed recently. Otherwise it just hands back
// whatever's already cached, no network call at all.
async fn refresh_before_flush(mut auth_token: Signal<Option<String>>) -> Option<String> {
    let storage = window().and_then(|w| w.local_storage().ok().flatten())?;
    if !should_refresh_now(&storage) {
        return storage.get_item("auth_token").ok().flatten().filter(|s| !s.is_empty());
    }
    let refresh_tok = storage.get_item("refresh_token").ok().flatten().filter(|s| !s.is_empty())?;
    match refresh_access_token(refresh_tok).await {
        Ok(auth) => {
            storage.set("auth_token", &auth.access_token).ok();
            storage.set("refresh_token", &auth.refresh_token).ok();
            mark_refreshed(&storage);
            auth_token.set(Some(auth.access_token.clone()));
            Some(auth.access_token)
        }
        Err(_) => storage.get_item("auth_token").ok().flatten().filter(|s| !s.is_empty()),
    }
}

// Whether a single queued op reached a real, final outcome (success, or a
// definitive server-side rejection/LWW-stale-drop — either way, nothing
// left to do for it) versus a retryable failure (couldn't reach the
// server at all). This distinction is what makes the flush loop crash-
// safe — see remove_front's doc comment in sync_queue.rs for the exact bug
// this replaced.
enum FlushOutcome {
    Done,
    Retry,
}

// Replays each queued op, in order, against the real Supabase API (the
// same functions SupabaseStorage itself delegates to — see api::storage.rs).
// Reads the queue via `peek` (never mutates it) and only calls
// `sync_queue::remove_front(1)` immediately after an op reaches a real
// outcome — success, or a definitive rejection worth dropping (delete_
// entity's FK-violation case, or an LWW-stale edit). The moment anything
// comes back as a retryable failure, this stops entirely: that op and
// everything queued behind it stays exactly as persisted, untouched, for
// the next tick — no attempt to guess what else might also be affected.
async fn flush_sync_queue(token: String, mut moments: Signal<Vec<MomentType>>, mut entities: Signal<Vec<EntityType>>, mut on_the_fly_task: Signal<Option<MomentType>>) {
    let ops = sync_queue::peek();
    // Seeded from every temp_id this device has ever resolved, not just
    // ones resolved earlier in *this* pass — see
    // sync_queue::RESOLVED_IDS's doc comment for why that distinction is
    // load-bearing.
    let mut id_map: HashMap<String, String> = sync_queue::RESOLVED_IDS.read().clone();

    for op in ops {
        let outcome = match op {
            QueuedOp::CreateMoment { temp_id, mut new } => {
                new.entity_id = remap_id(&new.entity_id, &id_map);
                match crate::api::moment::createMoment(new.clone(), token.clone()).await {
                    Ok(created) => {
                        id_map.insert(temp_id.clone(), created.id.clone());
                        sync_queue::RESOLVED_IDS.write().insert(temp_id.clone(), created.id.clone());
                        let mut all = synced_mirror::get_moments().unwrap_or_default();
                        match all.iter().position(|m| m.id == temp_id) {
                            Some(pos) => all[pos] = created.clone(),
                            None => all.push(created.clone()),
                        }
                        synced_mirror::set_moments(&all);
                        moments.write().retain(|m| m.id != temp_id);
                        moments.write().push(created.clone());
                        // "On the fly" (components/moment.rs's OnTheFlyCmp)
                        // caches its own snapshot of this same just-created
                        // moment, temp_id included, to render its "go do
                        // this now" screen and complete it on the immediate-
                        // flush trigger's schedule (task #58) reconciling
                        // the temp_id here — before the user got a chance to
                        // click Done — instead of getting the create and
                        // the completion-update into the same flush pass
                        // (where remap_id below would have handled it) —
                        // left this snapshot's id stale forever, so clicking
                        // Done queued an update against an id the server had
                        // never heard of. Silent no-op: the PATCH matched
                        // zero rows, still returned 200, so the op still
                        // reported success.
                        if on_the_fly_task.read().as_ref().is_some_and(|t| t.id == temp_id) {
                            on_the_fly_task.set(Some(created));
                        }
                        FlushOutcome::Done
                    }
                    Err(e) => {
                        clog!("Sync flush: create moment failed, will retry next tick ({})", e);
                        FlushOutcome::Retry
                    }
                }
            }
            QueuedOp::CreateEntity { temp_id, mut new } => {
                new.entity_type_id = new.entity_type_id.map(|t| remap_id(&t, &id_map));
                new.parent_entity_id = new.parent_entity_id.map(|p| remap_id(&p, &id_map));
                match crate::api::entity::createEntity(new.clone(), token.clone()).await {
                    Ok(created) => {
                        id_map.insert(temp_id.clone(), created.id.clone());
                        sync_queue::RESOLVED_IDS.write().insert(temp_id.clone(), created.id.clone());
                        let mut all = synced_mirror::get_entities().unwrap_or_default();
                        match all.iter().position(|e| e.id == temp_id) {
                            Some(pos) => all[pos] = created.clone(),
                            None => all.push(created.clone()),
                        }
                        synced_mirror::set_entities(&all);
                        entities.write().retain(|e| e.id != temp_id);
                        entities.write().push(created);
                        FlushOutcome::Done
                    }
                    Err(e) => {
                        clog!("Sync flush: create entity failed, will retry next tick ({})", e);
                        FlushOutcome::Retry
                    }
                }
            }
            QueuedOp::CreateReaction { temp_id, mut new } => {
                new.moment_id = remap_id(&new.moment_id, &id_map);
                match crate::api::moment::createReaction(new.clone(), token.clone()).await {
                    Ok(created) => {
                        id_map.insert(temp_id.clone(), created.id.clone());
                        sync_queue::RESOLVED_IDS.write().insert(temp_id.clone(), created.id.clone());
                        let mut all = synced_mirror::get_moments().unwrap_or_default();
                        if let Some(m) = all.iter_mut().find(|m| m.id == created.moment_id) {
                            let reactions = m.reactions.get_or_insert_with(Vec::new);
                            match reactions.iter().position(|r| r.id == temp_id) {
                                Some(pos) => reactions[pos] = created.clone(),
                                None => reactions.push(created.clone()),
                            }
                        }
                        synced_mirror::set_moments(&all);
                        // Reactions live nested inside each moment's own
                        // `reactions` field, not a top-level Signal of their
                        // own — the full refetch after this loop is what
                        // actually surfaces this in the UI.
                        FlushOutcome::Done
                    }
                    Err(e) => {
                        clog!("Sync flush: create reaction failed, will retry next tick ({})", e);
                        FlushOutcome::Retry
                    }
                }
            }
            QueuedOp::UpdateMomentField { id, field, value, staged_updated_at } => {
                let real_id = remap_id(&id, &id_map);
                match crate::api::moment::getMomentById(real_id.clone(), token.clone()).await {
                    Ok(Some(server_moment)) if is_stale(&staged_updated_at, &server_moment.updated_at) => {
                        // Someone else changed this moment on the server
                        // since we staged this edit — the server's newer
                        // version wins. Drop the edit and pull the newer
                        // row into the mirror instead of clobbering it.
                        clog!("Sync flush: server has a newer version of moment {}, dropping stale offline edit to '{}'", real_id, field);
                        let mut all = synced_mirror::get_moments().unwrap_or_default();
                        match all.iter().position(|m| m.id == real_id) {
                            Some(pos) => all[pos] = server_moment,
                            None => all.push(server_moment),
                        }
                        synced_mirror::set_moments(&all);
                        FlushOutcome::Done
                    }
                    Ok(_) => {
                        let real_value = remap_value(value.clone(), &id_map);
                        match crate::api::moment::update_moment_field(real_id, &field, real_value, token.clone()).await {
                            Ok(()) => FlushOutcome::Done,
                            Err(e) => {
                                clog!("Sync flush: update moment field failed, will retry next tick ({})", e);
                                FlushOutcome::Retry
                            }
                        }
                    }
                    // A genuine server rejection (most commonly: `real_id`
                    // never actually resolved to a real id — the create it
                    // depended on had already been reconciled in an earlier,
                    // separate flush pass, so this pass's own id_map had
                    // nothing to remap it with — see remap_id's doc comment)
                    // can never succeed no matter how many times it's
                    // retried. Drop it instead of hammering this endpoint
                    // forever — confirmed live: this exact op, stuck on a
                    // stale id, retried every single tick indefinitely.
                    Err(crate::api::StorageError::Remote(e)) => {
                        clog!("Sync flush: server rejected conflict check for moment {}, dropping this edit to '{}' ({})", real_id, field, e);
                        FlushOutcome::Done
                    }
                    Err(e) => {
                        clog!("Sync flush: couldn't check moment {} for conflicts, will retry next tick ({})", real_id, e);
                        FlushOutcome::Retry
                    }
                }
            }
            QueuedOp::UpdateEntityField { id, field, value, staged_updated_at } => {
                let real_id = remap_id(&id, &id_map);
                match crate::api::entity::getEntityById(real_id.clone(), token.clone()).await {
                    Ok(Some(server_entity)) if is_stale(&staged_updated_at, &server_entity.updated_at) => {
                        clog!("Sync flush: server has a newer version of entity {}, dropping stale offline edit to '{}'", real_id, field);
                        let mut all = synced_mirror::get_entities().unwrap_or_default();
                        match all.iter().position(|e| e.id == real_id) {
                            Some(pos) => all[pos] = server_entity,
                            None => all.push(server_entity),
                        }
                        synced_mirror::set_entities(&all);
                        FlushOutcome::Done
                    }
                    Ok(_) => {
                        let real_value = remap_value(value.clone(), &id_map);
                        match crate::api::entity::update_entity_field(real_id, &field, real_value, token.clone()).await {
                            Ok(()) => FlushOutcome::Done,
                            Err(e) => {
                                clog!("Sync flush: update entity field failed, will retry next tick ({})", e);
                                FlushOutcome::Retry
                            }
                        }
                    }
                    // See the matching UpdateMomentField arm above — same
                    // reasoning, same fix.
                    Err(crate::api::StorageError::Remote(e)) => {
                        clog!("Sync flush: server rejected conflict check for entity {}, dropping this edit to '{}' ({})", real_id, field, e);
                        FlushOutcome::Done
                    }
                    Err(e) => {
                        clog!("Sync flush: couldn't check entity {} for conflicts, will retry next tick ({})", real_id, e);
                        FlushOutcome::Retry
                    }
                }
            }
            QueuedOp::DeleteMoment(mut m) => {
                m.id = remap_id(&m.id, &id_map);
                match crate::api::moment::deleteMoment(m.clone(), token.clone()).await {
                    Ok(()) => FlushOutcome::Done,
                    Err(e) => {
                        clog!("Sync flush: delete moment failed, will retry next tick ({})", e);
                        FlushOutcome::Retry
                    }
                }
            }
            QueuedOp::DeleteEntity(id) => {
                let real_id = remap_id(&id, &id_map);
                match crate::api::entity::deleteEntity(real_id, token.clone()).await {
                    Ok(()) => FlushOutcome::Done,
                    Err(crate::api::StorageError::Remote(e)) => {
                        clog!("Sync flush: delete entity rejected by server, dropping ({})", e);
                        FlushOutcome::Done
                    }
                    Err(e) => {
                        clog!("Sync flush: delete entity couldn't reach the server, will retry next tick ({})", e);
                        FlushOutcome::Retry
                    }
                }
            }
            QueuedOp::DeleteReaction(mut r) => {
                r.id = remap_id(&r.id, &id_map);
                r.moment_id = remap_id(&r.moment_id, &id_map);
                match crate::api::moment::deleteReaction(r.clone(), token.clone()).await {
                    Ok(()) => FlushOutcome::Done,
                    Err(e) => {
                        clog!("Sync flush: delete reaction failed, will retry next tick ({})", e);
                        FlushOutcome::Retry
                    }
                }
            }
            QueuedOp::RestoreMoment(id) => {
                let real_id = remap_id(&id, &id_map);
                match crate::api::moment::restoreMoment(real_id, token.clone()).await {
                    Ok(()) => FlushOutcome::Done,
                    Err(e) => {
                        clog!("Sync flush: restore moment failed, will retry next tick ({})", e);
                        FlushOutcome::Retry
                    }
                }
            }
        };

        match outcome {
            FlushOutcome::Done => sync_queue::remove_front(1),
            FlushOutcome::Retry => break,
        }
    }

    refresh_mirror_and_signals(token, moments, entities).await;
}

// Re-fetches the full vault over the network and overwrites both the
// mirror and the live Signals with the result — this is what keeps the UI
// fresh once back online (SupabaseStorage's own read methods can't do this
// themselves: they're a cheap value constructed fresh per call, with no
// Signal handles at all — see storage.rs). Run after the queue replay
// above so this device's own just-flushed writes are already reflected in
// what comes back, not overwritten a moment later.
//
// Every create goes through the queue and isn't actually sent to the
// server until a flush tick picks it up (see SupabaseStorage::create_moment/
// create_entity) — including one created moments ago while fully online,
// still waiting for the *next* tick. If that happens while this tick's own
// fetch is in flight (or was already snapshotted just before), the
// server's response has no idea the new record exists yet. Wholesale-
// overwriting the mirror/Signal with that response would wipe the
// still-optimistic record from view until a later tick actually creates it
// server-side — confirmed live: add a moment, watch it vanish a couple
// seconds later, refresh and it's back (because by then a later tick
// really had created it). Nothing was ever lost — the queue still had it
// the whole time — but the UI lied about it in the meantime. Re-adding
// anything still referenced by a currently-queued create keeps the
// optimistic view stable across this overwrite; once that create actually
// flushes, it drops out of `pending` on its own and the server's copy (with
// its real id) takes over normally.
async fn refresh_mirror_and_signals(token: String, mut moments: Signal<Vec<MomentType>>, mut entities: Signal<Vec<EntityType>>) {
    let pending = sync_queue::peek();
    let pending_moment_ids: HashSet<String> = pending.iter().filter_map(|op| match op {
        QueuedOp::CreateMoment { temp_id, .. } => Some(temp_id.clone()),
        _ => None,
    }).collect();
    let pending_entity_ids: HashSet<String> = pending.iter().filter_map(|op| match op {
        QueuedOp::CreateEntity { temp_id, .. } => Some(temp_id.clone()),
        _ => None,
    }).collect();

    // Anything with an update/delete/restore still queued against it (not
    // yet confirmed by the server) would otherwise get clobbered back to
    // its stale, pre-edit state by this very refetch — completing a moment,
    // for instance, optimistically sets completed_at locally, but if this
    // refresh's own network round-trip is already in flight (or starts)
    // before that completion has actually been sent, the server's answer
    // still says "not completed," and blindly overwriting with it undoes
    // the completion in the UI until a *later* tick actually sends the real
    // update — confirmed live: complete a moment, watch it flash back into
    // the list a moment later, then disappear again once the real update
    // lands. `id` here may still be a client-minted temp_id if the create
    // it's queued behind hasn't been remapped by *this* pass — see
    // sync_queue::RESOLVED_IDS's doc comment — so it's remapped the same
    // way flush_sync_queue's own per-op handling does, before comparing
    // against the freshly fetched (always-real-id) rows below.
    let resolved = sync_queue::RESOLVED_IDS.read().clone();
    let remap = |id: &str| resolved.get(id).cloned().unwrap_or_else(|| id.to_string());
    let pending_moment_edit_ids: HashSet<String> = pending.iter().filter_map(|op| match op {
        QueuedOp::UpdateMomentField { id, .. } => Some(remap(id)),
        QueuedOp::DeleteMoment(m) => Some(remap(&m.id)),
        QueuedOp::RestoreMoment(id) => Some(remap(id)),
        _ => None,
    }).collect();
    let pending_entity_edit_ids: HashSet<String> = pending.iter().filter_map(|op| match op {
        QueuedOp::UpdateEntityField { id, .. } => Some(remap(id)),
        QueuedOp::DeleteEntity(id) => Some(remap(id)),
        _ => None,
    }).collect();

    if let Ok(mut open) = crate::api::moment::getMoments(token.clone()).await {
        if let Ok(deleted) = crate::api::moment::getDeletedMoments(token.clone()).await {
            open.extend(deleted);
        }
        // The mirror already holds this device's latest optimistic edit —
        // every write path (update_moment_field, delete_moment,
        // restore_moment) patches it immediately, before queuing — so
        // falling back to that instead of the freshly fetched row keeps
        // this stable regardless of what this refresh's own timing happens
        // to race against. Moments are only ever soft-deleted (deleted_at
        // set/cleared, never removed outright), so the mirror always still
        // has a row to fall back to here — unlike entities below.
        if !pending_moment_edit_ids.is_empty() {
            let mirror = synced_mirror::get_moments().unwrap_or_default();
            for row in open.iter_mut() {
                if pending_moment_edit_ids.contains(&row.id) {
                    if let Some(local) = mirror.iter().find(|m| m.id == row.id) {
                        *row = local.clone();
                    }
                }
            }
        }
        let still_local: Vec<MomentType> = moments.read().iter()
            .filter(|m| pending_moment_ids.contains(&m.id))
            .cloned()
            .collect();
        open.extend(still_local);
        synced_mirror::set_moments(&open);
        moments.set(open.into_iter().filter(|m| m.deleted_at.is_none()).collect());
    }
    if let Ok(mut fresh) = crate::api::entity::getEntities(token.clone()).await {
        // Same reasoning as moments above, except delete_entity actually
        // removes the row from the mirror outright (a real hard delete,
        // not soft) — so a queued-but-not-yet-flushed entity delete has no
        // mirror row to fall back to. In that case the mirror's *absence*
        // of the row is itself the correct local state, so it's dropped
        // from the fresh server list too, rather than kept as a stale
        // not-yet-deleted row.
        if !pending_entity_edit_ids.is_empty() {
            let mirror = synced_mirror::get_entities().unwrap_or_default();
            fresh.retain(|e| !pending_entity_edit_ids.contains(&e.id) || mirror.iter().any(|m| m.id == e.id));
            for row in fresh.iter_mut() {
                if pending_entity_edit_ids.contains(&row.id) {
                    if let Some(local) = mirror.iter().find(|m| m.id == row.id) {
                        *row = local.clone();
                    }
                }
            }
        }
        let still_local: Vec<EntityType> = entities.read().iter()
            .filter(|e| pending_entity_ids.contains(&e.id))
            .cloned()
            .collect();
        fresh.extend(still_local);
        synced_mirror::set_entities(&fresh);
        entities.set(fresh);
    }
    if let Ok(fresh_types) = crate::api::entity::getEntityTypes(token).await {
        synced_mirror::set_entity_types(&fresh_types);
    }
}


// #[cfg(target_arch = "wasm32")]
// fn window_size() -> (f64, f64) {
//     use web_sys::window;
//     let w = window().unwrap();
//     let width = w.inner_width().unwrap().as_f64().unwrap();
//     let height = w.inner_height().unwrap().as_f64().unwrap();
//     (width, height)
// }
// App-wide keyboard shortcuts (2026-07-29): "n" focuses the moment composer,
// "o" opens On the fly, Escape closes the activity panel. A plain Dioxus
// onkeydown on some wrapping element can't do this — keydown bubbles from
// the focused element *up* the DOM, so with nothing focused (the common
// idle-browsing case, where the focused element defaults to <body>) it
// would never reach a handler on a descendant div at all. document::eval
// (same primitive graph.rs already uses for its d3 interop, cross-platform
// across web and desktop) instead registers a real window-level listener in
// JS once and streams every keydown back over the eval's channel, so this
// works regardless of what currently has focus.
// Mobile-vs-desktop viewport detection (2026-08-01) — see AppState::
// is_desktop_viewport's doc comment for why this is plain window.innerWidth/
// innerHeight in JS rather than a Tailwind CSS breakpoint. `resize` alone
// doesn't cover an iOS Safari address-bar show/hide changing innerHeight
// without the window itself resizing, but this is the same signal a real
// resize event would carry, so a first reading, before ever exercising this
// codepath, wasn't judged worth a separate visualViewport listener.
const VIEWPORT_SCRIPT: &str = r#"
    function send() {
        dioxus.send({ width: window.innerWidth, height: window.innerHeight });
    }
    window.addEventListener('resize', send);
    send();
"#;

#[derive(serde::Deserialize, Clone)]
struct ViewportSize {
    width: f64,
    height: f64,
}

// Local-timezone offset (2026-08-01) — see AppState::local_utc_offset_minutes's
// doc comment. A one-shot read, not a live listener like VIEWPORT_SCRIPT
// above: unlike window size, a browser's timezone offset changing mid-
// session (a DST transition, or the user's system clock changing timezone)
// is rare enough not to warrant polling for.
const TIMEZONE_OFFSET_SCRIPT: &str = r#"
    dioxus.send(new Date().getTimezoneOffset());
"#;

// AppState::is_touch_device's doc comment — a one-shot read, same posture
// as TIMEZONE_OFFSET_SCRIPT above (a device either has a touch pointer or
// it doesn't; unlike window size this isn't something that changes mid-
// session). maxTouchPoints is the fallback for browsers where
// 'ontouchstart' was never reliable (older desktop Chrome briefly exposed
// it on some hybrid laptops); either signal being truthy is enough.
const TOUCH_CAPABLE_SCRIPT: &str = r#"
    dioxus.send('ontouchstart' in window || navigator.maxTouchPoints > 0);
"#;

// AppState::pwa_standalone's doc comment — one-shot, same posture as
// TOUCH_CAPABLE_SCRIPT (whether the app is already installed doesn't
// change without a full relaunch, which is a fresh page load anyway).
const STANDALONE_CHECK_SCRIPT: &str = r#"
    dioxus.send(window.matchMedia('(display-mode: standalone)').matches || window.navigator.standalone === true);
"#;

// AppState::pwa_install_available's doc comment. A live listener, not a
// one-shot read — unlike touch capability, whether the browser is willing
// to prompt for install can genuinely change *during* the session (Chrome
// fires `beforeinstallprompt` some time after load, on its own schedule,
// not necessarily before this even registers) and can go away again once
// used (`appinstalled`, or the captured event just going stale after its
// single allowed `.prompt()` call). `preventDefault()` here is what stops
// Chrome from showing its own mini-infobar automatically — Settings owns
// presenting this now, not the browser.
const INSTALL_PROMPT_LISTENER_SCRIPT: &str = r#"
    window.addEventListener('beforeinstallprompt', (e) => {
        e.preventDefault();
        window.__bsbInstallPrompt = e;
        dioxus.send(true);
    });
    window.addEventListener('appinstalled', () => {
        window.__bsbInstallPrompt = null;
        dioxus.send(false);
    });
"#;

// The matching "fire this once, on demand" script — components::settings'
// install button — replaying the single captured `beforeinstallprompt`
// event (a browser API only allows calling `.prompt()` on it once ever) —
// is defined locally there instead of here; see its own comment for why.

#[derive(serde::Deserialize, Clone)]
struct GlobalKeyEvent {
    key: String,
    ctrl: bool,
    meta: bool,
    alt: bool,
    tag: String,
    editable: bool,
}

const GLOBAL_KEYDOWN_SCRIPT: &str = r#"
    window.addEventListener('keydown', (e) => {
        const el = document.activeElement;
        const tag = el ? el.tagName : '';
        const editable = el ? !!el.isContentEditable : false;
        const isTyping = editable || tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT';
        const noModifiers = !e.ctrlKey && !e.metaKey && !e.altKey;
        const lower = e.key.toLowerCase();
        // Stop the browser's own default (typing the letter into whatever
        // ends up focused) before it can happen at all — deciding this here,
        // synchronously in the same listener that captured the key, is the
        // only way to make it race-proof. Rust's focus_composer/on_the_fly
        // handling below runs asynchronously (a signal update + re-render +
        // effect), and by the time it actually calls set_focus, the browser
        // may already be partway through inserting this same keystroke into
        // the element focus just landed on — that's what caused "n" to both
        // focus the composer AND type a literal "n" into it.
        if (noModifiers && !isTyping && (lower === 'n' || lower === 'o')) {
            e.preventDefault();
        }
        dioxus.send({
            key: e.key,
            ctrl: e.ctrlKey,
            meta: e.metaKey,
            alt: e.altKey,
            tag: tag,
            editable: editable,
        });
    });
"#;

#[component]
pub fn Sidebar() -> Element {
    let state = use_context::<AppState>();
    let mut sidebarTgl = state.sidebarTgl;
    rsx! {
        div {
            // pb-[200px] (here and on the desktop sidebar/activity bar
            // below) — scrolled-to-bottom content used to sit flush
            // against the viewport edge, and the fixed On the fly button
            // (bottom-6 left-6) sat right on top of whatever was there.
            class: if *sidebarTgl.read() {
                "fixed top-0 left-0 h-full overflow-y-auto pb-[200px] w-64 shadow-xl z-40 transform translate-x-0 transition-transform duration-200 bg-background border-r border-border"
            } else {
                "fixed top-0 left-0 h-full overflow-y-auto pb-[200px] w-64 shadow-xl z-40 transform -translate-x-full transition-transform duration-200 bg-background border-r border-border"
            },
            div {
                class:"h-1",
            }
            vault_switcher_cmp { }
            div {
                class: "px-3 mt-1 pt-4 border-t border-border",
                span {
                    class: "block px-3 mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground",
                    "Views"
                }
                views_list_cmp { }
            }
            entity_list_cmp { }
            project_list_cmp { }
            tag_list_cmp { }
        }
    }
}

// One entry in the vault switcher's dropdown. Deliberately list-shaped (a
// Vec built fresh on every render) rather than hardcoded menu items, even
// though today it only ever holds "Local" plus optionally "Synced" — see
// memory reference_local_first_pivot_plan. Extending to a real open-ended
// multi-vault list later is then just "populate this Vec from a stored
// registry instead," not a UI rewrite.
struct VaultEntry {
    kind: VaultKind,
    label: String,
    // Local is the app's one-and-only local vault right now (no multi-vault
    // support yet), so it's never removable. Synced is removable — "remove"
    // just means log out, there's nothing to delete server-side here.
    removable: bool,
}

#[component]
pub fn vault_switcher_cmp() -> Element {
    let state = use_context::<AppState>();
    let mut auth_token = state.auth_token;
    let mut user_id = state.user_id;
    let mut user_email = state.user_email;
    let mut active_vault = state.active_vault;
    let mut sidebarTgl = state.sidebarTgl;
    let mut backdropTgl = state.backdropTgl;
    let mut currentView = state.currentView;
    let mut current_entity = state.current_entity;
    let is_desktop_viewport = state.is_desktop_viewport;
    let is_touch_device = state.is_touch_device;
    let mut confirming_removal_of = use_signal(|| None::<VaultKind>);
    // The popup Dropdown below only actually works with a mouse — see its
    // own comment further down. is_desktop_viewport alone (window size
    // only) put iPad in the same bucket as a real desktop despite being
    // touch, so this also has to check is_touch_device (AppState) directly,
    // not just viewport width.
    let use_popup_switcher = *is_desktop_viewport.read() && !*is_touch_device.read();

    // Keyed off auth_token, not user_email — user_email is only populated
    // once the mount-time session check (see the effect below) actually
    // completes a round trip, which can legitimately take a while or never
    // resolve at all while offline. auth_token is restored synchronously
    // from localStorage at startup (main.rs), so it's the real signal for
    // "is there a Synced session," not a proxy for "have we successfully
    // fetched the display name yet." Using user_email here used to mean a
    // slow/offline first check left a real session sitting there while
    // this UI insisted no Synced vault existed — "+ Add a vault" would
    // show, but clicking it just bounced straight back off Login's own
    // already-logged-in redirect (auth_token being genuinely Some).
    let entries: Vec<VaultEntry> = {
        let mut v = vec![VaultEntry { kind: VaultKind::Local, label: "Local".to_string(), removable: false }];
        if auth_token.read().is_some() {
            let label = user_email.read().clone().unwrap_or_else(|| "Synced".to_string());
            v.push(VaultEntry { kind: VaultKind::Synced, label, removable: true });
        }
        v
    };
    let has_synced = auth_token.read().is_some();

    // See VaultKind::effective's doc comment — the raw signal can say
    // "Synced" even on a first-ever, never-logged-in visit, so the switcher
    // has to normalize it the same way the storage layer does, or it'll
    // claim to be on a vault that isn't even in its own list.
    let current = active_vault.read().effective(&auth_token.read());
    let current_label = entries.iter().find(|e| e.kind == current)
        .map(|e| e.label.clone())
        .unwrap_or_else(|| "Local".to_string());
    let initial = current_label.chars().next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string());

    let mut select_vault = move |kind: VaultKind| {
        active_vault.set(kind);
        // Desktop has no preference persistence yet (see main.rs's startup
        // effect) — web_sys::window() panics on a native target rather than
        // just returning None, so this has to be compiled out entirely
        // rather than relying on the `if let Some(...)` to fail gracefully.
        #[cfg(not(feature = "desktop"))]
        if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
            storage.set("active_vault", kind.as_storage_str()).ok();
        }
    };

    // "Removing" the Synced vault is just logging out — there's one account,
    // so there's nothing else to delete. Falls back to Local if Synced was
    // the active vault, so the switcher never points at a vault that no
    // longer exists.
    let mut remove_synced = move || {
        #[cfg(not(feature = "desktop"))]
        if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
            // remove_item, not set("", ...) — an empty string is still a
            // present value, and used to read back on the next app launch
            // as Some("") (see main.rs's startup restore), which every
            // auth_token.is_some() check treats as "logged in" despite
            // there being no real session. remove_item leaves nothing to
            // misread.
            storage.remove_item("auth_token").ok();
            storage.remove_item("refresh_token").ok();
            storage.remove_item(LAST_REFRESHED_AT_KEY).ok();
        }
        // This account's mirror/queue (see api::synced_mirror/sync_queue)
        // belong to this session specifically — clear them here too, or a
        // later "+ Add a vault" login (same browser, maybe a different
        // account) would see the previous account's stale cached data
        // before its first real fetch. Was missing here previously (only
        // components/settings.rs's own separate remove-vault action did
        // this) — two independent logout entry points had silently
        // diverged.
        crate::api::synced_mirror::clear();
        crate::api::sync_queue::clear();
        auth_token.set(None);
        user_id.set(None);
        user_email.set(None);
        if *active_vault.read() == VaultKind::Synced {
            select_vault(VaultKind::Local);
        }
    };

    rsx! {
        // On any touch device (phones, and iPads — 2026-07-23, then again
        // 2026-08-03 once is_touch_device could actually tell an iPad apart
        // from a wide desktop window instead of guessing off viewport size
        // alone) the Dropdown version below is unusable: tapping any item
        // inside it closes the menu instead of selecting it, near-certainly
        // the same class of bug already root-caused for the right-click
        // ContextMenu (see components/context_menu/component.rs) — a
        // touch's pointerdown firing the library's outside-dismiss handler
        // before its own click/tap handler gets a chance to run. Rather
        // than chase that down inside a shared lumen_blocks component too,
        // this sidesteps it entirely on touch devices: no popup, just
        // always-visible rows. Revisit properly if this turns out to
        // matter for more than vault-switching.
        div {
            class: if use_popup_switcher { "hidden px-3 flex flex-col gap-y-0.5" } else { "px-3 flex flex-col gap-y-0.5" },
            for entry in entries.iter() {
                {
                    let kind = entry.kind;
                    let is_current = kind == current;
                    let label = entry.label.clone();
                    rsx! {
                        a {
                            key: "{label}",
                            class: "flex items-center gap-2 rounded-md px-2 py-2 hover:bg-muted transition-colors cursor-pointer w-full text-sm font-medium text-foreground",
                            onclick: move |_| select_vault(kind),
                            span { class: "truncate", if is_current { "✓ " } else { "" } "{label}" }
                        }
                    }
                }
            }
            if !has_synced {
                a {
                    class: "rounded-md px-2 py-2 hover:bg-muted transition-colors cursor-pointer w-full text-sm text-muted-foreground hover:text-foreground",
                    onclick: move |_| {
                        sidebarTgl.set(false);
                        backdropTgl.set(false);
                        navigator().push(Route::LoginCMP {});
                    },
                    "+ Add a vault"
                }
            }
            if has_synced {
                a {
                    class: "rounded-md px-2 py-2 hover:bg-muted transition-colors cursor-pointer w-full text-sm text-muted-foreground hover:text-foreground",
                    onclick: move |_| {
                        if *confirming_removal_of.read() == Some(VaultKind::Synced) {
                            remove_synced();
                            confirming_removal_of.set(None);
                        } else {
                            confirming_removal_of.set(Some(VaultKind::Synced));
                        }
                    },
                    if *confirming_removal_of.read() == Some(VaultKind::Synced) { "Tap again to remove Synced vault" } else { "Remove Synced vault" }
                }
            }
            a {
                class: "rounded-md px-2 py-2 hover:bg-muted transition-colors cursor-pointer w-full text-sm font-medium text-foreground",
                onclick: move |_| {
                    current_entity.set(None);
                    currentView.set(View::Settings);
                },
                "Settings"
            }
        }
        div {
            class: if use_popup_switcher { "block px-3" } else { "hidden px-3" },
            div {
                class: "w-full sidebar-vault-switcher",
                Dropdown {
                    DropdownTrigger {
                        class: "w-full text-left",
                        div {
                            class: "flex items-center gap-2 rounded-md px-2 py-2 hover:bg-muted transition-colors cursor-pointer w-full",
                            Avatar {
                                class: "h-8 w-8 shrink-0",
                                AvatarFallback { class: "text-sm", "{initial}" }
                            }
                            span {
                                class: "text-sm font-medium text-foreground truncate",
                                "{current_label}"
                            }
                        }
                    }
                    DropdownContent {
                        align: "start",
                        for (idx, entry) in entries.iter().enumerate() {
                            {
                                let kind = entry.kind;
                                let is_current = kind == current;
                                let removable = entry.removable;
                                let label = entry.label.clone();
                                rsx! {
                                    DropdownItem::<String> {
                                        value: label.clone(),
                                        index: idx,
                                        on_select: move |_| select_vault(kind),
                                        div {
                                            class: "flex items-center justify-between gap-2 w-full",
                                            span {
                                                class: "truncate",
                                                if is_current { "✓ " } else { "" }
                                                "{label}"
                                            }
                                            if removable {
                                                if *confirming_removal_of.read() == Some(kind) {
                                                    span {
                                                        class: "text-xs font-medium text-destructive hover:underline px-1 shrink-0",
                                                        onclick: move |e| {
                                                            e.stop_propagation();
                                                            remove_synced();
                                                            confirming_removal_of.set(None);
                                                        },
                                                        "Remove"
                                                    }
                                                } else {
                                                    span {
                                                        class: "text-muted-foreground hover:text-foreground px-1 shrink-0",
                                                        title: "Vault settings",
                                                        onclick: move |e| {
                                                            e.stop_propagation();
                                                            confirming_removal_of.set(Some(kind));
                                                        },
                                                        "⋯"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if !has_synced {
                            DropdownSeparator {}
                            DropdownItem::<String> {
                                value: "add".to_string(),
                                index: entries.len(),
                                on_select: move |_| {
                                    // On mobile the sidebar is a fixed-position drawer
                                    // with a full-screen dimming backdrop (z-30) above
                                    // the routed content. Navigating away without
                                    // closing both left the backdrop up, blocking and
                                    // hiding the login page underneath it — "the
                                    // dropdown closes and I don't get to log in".
                                    sidebarTgl.set(false);
                                    backdropTgl.set(false);
                                    navigator().push(Route::LoginCMP {});
                                },
                                "+ Add a vault"
                            }
                        }
                        DropdownSeparator {}
                        DropdownItem::<String> {
                            value: "settings".to_string(),
                            index: entries.len() + 1,
                            on_select: move |_| {
                                current_entity.set(None);
                                currentView.set(View::Settings);
                            },
                            "Settings"
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn Navbar() -> Element {
    let state = use_context::<AppState>();
    let mut activity_bar_tgl = state.activity_bar_tgl;
    let activity_bar_view = state.activity_bar_view;
    let mut momentInputTgl = state.momentInputTgl;
    let mut backdropTgl = state.backdropTgl;
    let mut sidebarTgl = state.sidebarTgl;
    let current_moment = state.current_moment;
    let current_view = state.currentView;
    let current_entity = state.current_entity;
    let mut auth_token = state.auth_token;
    let mut user_id = state.user_id;
    let mut user_email = state.user_email;
    let mut active_vault = state.active_vault;
    let mut focus_composer = state.focus_composer;
    let mut on_the_fly_open = state.on_the_fly_open;
    let mut refresh_loop_started = use_signal(|| false);
    let mut session_check_started = use_signal(|| false);
    let mut keyboard_listener_started = use_signal(|| false);
    let mut viewport_listener_started = use_signal(|| false);
    let mut is_desktop_viewport = state.is_desktop_viewport;
    let mut sidebar_collapsed = state.sidebar_collapsed;
    let mut timezone_offset_read = use_signal(|| false);
    let mut touch_capability_read = use_signal(|| false);
    let mut is_touch_device = state.is_touch_device;
    let mut standalone_check_read = use_signal(|| false);
    let mut pwa_standalone = state.pwa_standalone;
    let mut install_listener_started = use_signal(|| false);
    let mut pwa_install_available = state.pwa_install_available;
    let mut local_utc_offset_minutes = state.local_utc_offset_minutes;
    let moments = state.moments;
    let entities = state.entities;
    let on_the_fly_task = state.on_the_fly_task;
    let mut sync_flush_loop_started = use_signal(|| false);
    let mut sync_online_listener_started = use_signal(|| false);
    let mut sync_flushing = use_signal(|| false);
    let moment = current_moment.read().clone();

    // Offline-first sync for the Synced vault (see api::sync_queue/
    // synced_mirror) — periodic tick, same TimeoutFuture shape as the
    // token-refresh loop below, at a much shorter interval so a reconnect
    // doesn't have to wait up to an hour to actually flush. `sync_flushing`
    // guards against the periodic tick and the 'online' listener below
    // both firing a flush at once.
    use_effect(move || {
        if *sync_flush_loop_started.read() {
            return;
        }
        sync_flush_loop_started.set(true);
        spawn(async move {
            loop {
                TimeoutFuture::new(SYNC_FLUSH_INTERVAL_MS).await;
                if auth_token.read().is_none() || *sync_flushing.read() {
                    continue;
                }
                sync_flushing.set(true);
                if let Some(token) = refresh_before_flush(auth_token).await {
                    flush_sync_queue(token, moments, entities, on_the_fly_task).await;
                }
                sync_flushing.set(false);
            }
        });
    });

    // Flushes immediately on reconnect instead of waiting for the next
    // periodic tick above — same eval-listener pattern as
    // VIEWPORT_SCRIPT/GLOBAL_KEYDOWN_SCRIPT.
    use_effect(move || {
        if *sync_online_listener_started.read() {
            return;
        }
        sync_online_listener_started.set(true);
        spawn(async move {
            let mut eval = document::eval(ONLINE_SCRIPT);
            while let Ok(_) = eval.recv::<bool>().await {
                if auth_token.read().is_none() || *sync_flushing.read() {
                    continue;
                }
                sync_flushing.set(true);
                if let Some(token) = refresh_before_flush(auth_token).await {
                    flush_sync_queue(token, moments, entities, on_the_fly_task).await;
                }
                sync_flushing.set(false);
            }
        });
    });

    // Flushes right after something's actually queued, instead of waiting
    // up to SYNC_FLUSH_INTERVAL_MS for the periodic tick — the common case
    // (genuinely online) shouldn't have to wait a rhythm out. The periodic
    // tick and 'online' listener above stay exactly as they were: this
    // doesn't replace the fallback, it just usually beats it to the punch.
    // `sync_queue::FLUSH_REQUESTED` (a plain counter, bumped by every
    // `push`) has to be read synchronously here, not just inside `spawn`,
    // for Dioxus to track it as this effect's dependency — same reasoning
    // as the mount-time session check's own doc comment above.
    //
    // auth_token/sync_flushing are read via `.peek()`, not `.read()`, here
    // — deliberately NOT tracked as reactive dependencies of this effect,
    // unlike FLUSH_REQUESTED. This one bit us badly (2026-08-07): with
    // `.read()`, this effect was ALSO subscribed to sync_flushing's own
    // changes — and this same effect body is what WRITES sync_flushing
    // (true right before spawning, false once the flush finishes). Every
    // one of those writes re-triggered this exact effect, which flushed
    // again, which wrote sync_flushing again, forever — a genuine infinite
    // loop, confirmed live via diagnostic logging: it fired continuously,
    // roughly every 200-300ms, indefinitely, with FLUSH_REQUESTED sitting
    // unchanged at 0 the entire time (nothing was even being queued — an
    // idle app was still hammering Supabase with a full getMoments/
    // getEntities/getEntityTypes refetch several times a second). That's
    // what was actually behind two seemingly unrelated bug reports: a
    // moment's completion flickering (disappear/reappear/disappear as each
    // of these redundant refetches raced the real update and repeatedly
    // overwrote the moments Signal) and the entity graph view visibly
    // re-laying-out every fraction of a second (its own effect depends on
    // that same moments/entities Signal, so every one of these redundant
    // overwrites re-triggered it too). `.peek()` reads the current value
    // without subscribing, so writing to either signal elsewhere no longer
    // loops back into re-running this effect — its only real trigger is a
    // genuine new push.
    use_effect(move || {
        let _ = *sync_queue::FLUSH_REQUESTED.read();
        if auth_token.peek().is_none() || *sync_flushing.peek() {
            return;
        }
        sync_flushing.set(true);
        spawn(async move {
            if let Some(token) = refresh_before_flush(auth_token).await {
                flush_sync_queue(token, moments, entities, on_the_fly_task).await;
            }
            sync_flushing.set(false);
        });
    });

    // Started once per mounted session, same guard pattern as the keyboard
    // listener below — see VIEWPORT_SCRIPT/AppState::is_desktop_viewport for
    // why this exists instead of a CSS breakpoint.
    use_effect(move || {
        if *viewport_listener_started.read() {
            return;
        }
        viewport_listener_started.set(true);
        spawn(async move {
            let mut eval = document::eval(VIEWPORT_SCRIPT);
            while let Ok(size) = eval.recv::<ViewportSize>().await {
                is_desktop_viewport.set(size.width >= 500.0 || size.height >= 900.0);
                // Width-only, independent of the OR-based desktop check
                // above — a narrow-but-tall window still counts as
                // "desktop" there, but is still too cramped for a fixed
                // 256px docked sidebar.
                sidebar_collapsed.set(size.width < SIDEBAR_FOLD_WIDTH);
            }
        });
    });

    // One-shot, same guard pattern as above — see
    // AppState::local_utc_offset_minutes/TIMEZONE_OFFSET_SCRIPT.
    use_effect(move || {
        if *timezone_offset_read.read() {
            return;
        }
        timezone_offset_read.set(true);
        spawn(async move {
            let mut eval = document::eval(TIMEZONE_OFFSET_SCRIPT);
            if let Ok(offset) = eval.recv::<i32>().await {
                local_utc_offset_minutes.set(offset);
            }
        });
    });

    // One-shot, same guard pattern as above — see
    // AppState::is_touch_device/TOUCH_CAPABLE_SCRIPT.
    use_effect(move || {
        if *touch_capability_read.read() {
            return;
        }
        touch_capability_read.set(true);
        spawn(async move {
            let mut eval = document::eval(TOUCH_CAPABLE_SCRIPT);
            if let Ok(touch) = eval.recv::<bool>().await {
                is_touch_device.set(touch);
            }
        });
    });

    // One-shot, same guard pattern as above — see
    // AppState::pwa_standalone/STANDALONE_CHECK_SCRIPT.
    use_effect(move || {
        if *standalone_check_read.read() {
            return;
        }
        standalone_check_read.set(true);
        spawn(async move {
            let mut eval = document::eval(STANDALONE_CHECK_SCRIPT);
            if let Ok(standalone) = eval.recv::<bool>().await {
                pwa_standalone.set(standalone);
            }
        });
    });

    // Live listener, not one-shot — see INSTALL_PROMPT_LISTENER_SCRIPT's own
    // doc comment for why. Same started-once guard pattern as the other
    // listeners in this file (ONLINE_SCRIPT, GLOBAL_KEYDOWN_SCRIPT).
    use_effect(move || {
        if *install_listener_started.read() {
            return;
        }
        install_listener_started.set(true);
        spawn(async move {
            let mut eval = document::eval(INSTALL_PROMPT_LISTENER_SCRIPT);
            while let Ok(available) = eval.recv::<bool>().await {
                pwa_install_available.set(available);
            }
        });
    });

    // Started once per mounted session, same guard pattern as the token
    // refresh loop below.
    use_effect(move || {
        if *keyboard_listener_started.read() {
            return;
        }
        keyboard_listener_started.set(true);
        spawn(async move {
            let mut eval = document::eval(GLOBAL_KEYDOWN_SCRIPT);
            while let Ok(payload) = eval.recv::<GlobalKeyEvent>().await {
                if payload.key == "Escape" {
                    if *activity_bar_tgl.read() {
                        activity_bar_tgl.set(false);
                        backdropTgl.set(false);
                    }
                    continue;
                }
                let is_typing = payload.editable
                    || matches!(payload.tag.as_str(), "INPUT" | "TEXTAREA" | "SELECT");
                if payload.ctrl || payload.meta || payload.alt || is_typing {
                    continue;
                }
                match payload.key.to_lowercase().as_str() {
                    "n" => {
                        let current = *focus_composer.read();
                        focus_composer.set(current + 1);
                    }
                    "o" => on_the_fly_open.set(true),
                    _ => {}
                }
            }
        });
    });

    // On every load of the main app, confirm the token we have cached in
    // localStorage is still actually accepted by Supabase. A token can die
    // server-side (expiry, revocation) with no client-side signal, which
    // previously left the app showing an empty "logged in" shell. If the
    // token is dead, log the user out for real (clear storage + state) —
    // but land back on the (now-Local) app, not a dead-end login screen.
    // Login is opt-in now (see the vault switcher's "+ Add a vault"), never
    // something a bad cached token can strand you behind.
    //
    // `session_check_started` guards this the same way every other one-shot
    // effect in this file does — reading `auth_token` below makes Dioxus
    // track it as a dependency, so this whole effect body reruns any time
    // auth_token changes, including from the refresh a few lines down
    // setting it to the *new* token it just got. Without the guard, that
    // rerun spawned a second, fully independent check-and-maybe-refresh
    // task on top of whatever the first one was still doing. Confirmed
    // live: two "Session check rejected" log lines at the exact same
    // timestamp, from two concurrent calls both racing to refresh against
    // the same refresh_token — Supabase rotates it on every use, so
    // whichever call loses gets rejected using an already-spent token,
    // which (per Supabase's reuse-detection) can take the whole session
    // down with it. That's the real mechanism behind "moments/entities
    // just stop reaching the server" — every subsequent sync write then
    // fails auth forever, silently, against a token that never had a
    // chance to be valid for more than an instant.
    use_effect(move || {
        let Some(token) = auth_token.read().clone() else {
            return;
        };
        if *session_check_started.read() {
            return;
        }
        session_check_started.set(true);

        spawn(async move {
            match get_current_user(token).await {
                Ok(user) => {
                    user_email.set(Some(user.email));
                }
                // The request never reached Supabase at all (offline, or a
                // transient connectivity blip) — this says nothing about
                // whether the token is actually still valid, so it's not
                // grounds to log anyone out. Leaving the session exactly as
                // it is is what makes the Synced vault survive a page
                // refresh while offline (see api::synced_mirror/sync_queue)
                // — this used to collapse into the same "log out" path as
                // a real rejection below, which is what broke that.
                Err(crate::api::AuthError::Network(e)) => {
                    clog!("Session check couldn't reach the server (offline?), leaving session as-is: {}", e);
                }
                Err(crate::api::AuthError::Rejected(msg)) => {
                    // A real answer from Supabase saying this token is dead.
                    // This used to log straight out the moment the *cached*
                    // access token failed this check — which is nearly
                    // guaranteed to happen on every fresh page load after the
                    // browser's been closed a while (access tokens are
                    // short-lived, ~1hr), even though the much longer-lived
                    // refresh_token sitting right next to it in storage is
                    // still perfectly valid. The proactive 50-minute refresh
                    // loop below only helps a tab that's stayed open
                    // continuously — it does nothing for "closed the browser
                    // overnight, opened it again" — so this was the actual
                    // "losing my vault login" bug, not the loop. Try a real
                    // refresh first; only actually log out if that also
                    // comes back rejected (refresh_token itself expired/revoked).
                    clog!("Session check rejected ({}), attempting token refresh before logging out", msg);
                    #[cfg(not(feature = "desktop"))]
                    let should_log_out = 'refresh: {
                        let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) else {
                            break 'refresh false;
                        };
                        // No refresh_token to even try — unlike a network
                        // failure below, this isn't "maybe offline, leave
                        // it be": there's no path back to a valid session
                        // without one, so this is as final as an explicit
                        // rejection. Previously this returned `false` (same
                        // as the network-failure case), which left a
                        // rejected token with nothing to refresh it against
                        // sitting there forever — auth_token never became
                        // genuinely None, so Login's already-logged-in
                        // redirect guard would bounce away from the login
                        // page permanently, with no way back in.
                        let Some(refresh_tok) = storage.get_item("refresh_token").ok().flatten().filter(|s| !s.is_empty()) else {
                            break 'refresh true;
                        };
                        match refresh_access_token(refresh_tok).await {
                            Ok(auth) => {
                                storage.set("auth_token", &auth.access_token).ok();
                                storage.set("refresh_token", &auth.refresh_token).ok();
                                mark_refreshed(&storage);
                                auth_token.set(Some(auth.access_token));
                                user_id.set(Some(auth.user.id));
                                user_email.set(Some(auth.user.email));
                                false
                            }
                            // Couldn't even attempt the refresh due to
                            // connectivity — same reasoning as the outer
                            // Network arm, don't log out over this either.
                            Err(crate::api::AuthError::Network(e)) => {
                                clog!("Token refresh couldn't reach the server (offline?), leaving session as-is: {}", e);
                                false
                            }
                            Err(crate::api::AuthError::Rejected(e)) => {
                                clog!("Token refresh rejected too, logging out: {}", e);
                                true
                            }
                        }
                    };
                    #[cfg(feature = "desktop")]
                    let should_log_out = false;

                    if should_log_out {
                        // Deliberately NOT clearing the sync mirror/queue
                        // here (unlike the user-initiated "Remove Synced
                        // vault"/"Delete my account" flows in
                        // components/settings.rs) — this is a forced
                        // logout from a dead token, not the user
                        // disconnecting. Any not-yet-flushed offline edits
                        // stay queued so logging back into the same
                        // account still gets them synced instead of
                        // silently losing them.
                        #[cfg(not(feature = "desktop"))]
                        if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
                            storage.remove_item("auth_token").ok();
                            storage.remove_item("refresh_token").ok();
                            storage.set("active_vault", VaultKind::Local.as_storage_str()).ok();
                            storage.remove_item(LAST_REFRESHED_AT_KEY).ok();
                        }
                        auth_token.set(None);
                        user_id.set(None);
                        user_email.set(None);
                        active_vault.set(VaultKind::Local);
                    }
                }
            }
        });

        // Keep the session alive proactively so it doesn't reach the point
        // of dying in the first place. Started once per mounted session.
        // Desktop has no persisted refresh_token to act on (see main.rs's
        // startup effect) and web_sys::window() panics on a native target,
        // so there's nothing useful for this loop to do there yet.
        #[cfg(not(feature = "desktop"))]
        if !*refresh_loop_started.read() {
            refresh_loop_started.set(true);
            spawn(async move {
                loop {
                    TimeoutFuture::new(TOKEN_REFRESH_INTERVAL_MS).await;
                    let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) else {
                        break;
                    };
                    let Some(refresh_tok) = storage.get_item("refresh_token").ok().flatten().filter(|s| !s.is_empty()) else {
                        break;
                    };
                    // Skip if the flush loop (or this same loop, or the
                    // mount-time check) already refreshed recently — see
                    // should_refresh_now's doc comment. At a 50-minute
                    // interval this rarely actually matters, but keeps
                    // every refresh call site honoring the same one clock.
                    if !should_refresh_now(&storage) {
                        continue;
                    }
                    match refresh_access_token(refresh_tok).await {
                        Ok(auth) => {
                            storage.set("auth_token", &auth.access_token).ok();
                            storage.set("refresh_token", &auth.refresh_token).ok();
                            mark_refreshed(&storage);
                            auth_token.set(Some(auth.access_token));
                            user_id.set(Some(auth.user.id));
                        }
                        // Offline (or a transient blip) — this tick just
                        // couldn't reach Supabase at all, which says
                        // nothing about whether the refresh_token is
                        // actually still good. Skip this tick without
                        // logging out and, critically, without `break`ing
                        // the loop — it used to stop entirely here, so a
                        // long offline stretch meant this loop never ran
                        // again for the rest of the session even after
                        // reconnecting.
                        Err(crate::api::AuthError::Network(e)) => {
                            clog!("Proactive refresh couldn't reach the server (offline?), will retry next tick: {}", e);
                        }
                        Err(crate::api::AuthError::Rejected(e)) => {
                            clog!("Token refresh rejected, logging out: {}", e);
                            storage.remove_item("auth_token").ok();
                            storage.remove_item("refresh_token").ok();
                            storage.set("active_vault", VaultKind::Local.as_storage_str()).ok();
                            storage.remove_item(LAST_REFRESHED_AT_KEY).ok();
                            auth_token.set(None);
                            user_id.set(None);
                            user_email.set(None);
                            active_vault.set(VaultKind::Local);
                            break;
                        }
                    }
                }
            });
        }
    });
    let width_class = if *is_desktop_viewport.read() { "w-96" } else { "w-full" };
    let activity_bar_class = if *activity_bar_tgl.read() {
        format!("openedbtw fixed inset-y-0 right-0 z-[60] {width_class} transition-transform duration-300 translate-x-0")
    } else {
        format!("closedbtw fixed inset-y-0 right-0 z-[60] {width_class} transition-transform duration-300 translate-x-full")
    };
 
    //
    let header_title = match current_view.read().clone() {
        Inbox => "".to_string(),
        Entity => "".to_string(),
        Priority => "".to_string(),
        AllEntities => "".to_string(),
        Due => "".to_string(),
        Scheduled => "".to_string(),
        Blocking => "".to_string(),
        Notes => "".to_string(),
        Settings => "".to_string(),
        RecentlyDeleted => "".to_string(),
        SelfEntity => "".to_string(),
        Momentos => "".to_string(),
        Missed => "".to_string(),
    };

    rsx! {

        button {
            // Explicit h-12/w-12 (48px) hit-box, not just the glyph's own
            // font-size — a bare text-2xl "☰" with no box was a ~24px tap
            // target, well under the ~44px minimum comfortable touch size.
            // Also shown (not just on mobile) when the docked desktop
            // sidebar has auto-folded for width — otherwise there'd be no
            // way to reach it at all at that width. Reuses the same
            // mobile drawer (`Sidebar` component, driven by sidebarTgl) as
            // a fallback overlay rather than a second sidebar
            // implementation.
            class: if *is_desktop_viewport.read() && !*sidebar_collapsed.read() { "hidden" } else { "fixed flex items-center justify-center z-51 left-3 top-1 h-12 w-12 text-3xl rounded-md active:bg-foreground/10 transition-colors" },
            onclick: move |_| {
                let tgl = *sidebarTgl.read();
                sidebarTgl.set(!tgl);
                clog!("clicked hamburger");
            },
            if *sidebarTgl.read() { "" } else { "☰" }
        }


        if *backdropTgl.read() || *sidebarTgl.read() || *momentInputTgl.read() {
            div {
                id: "backdrop",
                class: if *is_desktop_viewport.read() && !*sidebar_collapsed.read() { "hidden" } else { "fixed inset-0 bg-black/20 z-30" },
                onclick: move |_| {
                    clog!("clicked");
                    momentInputTgl.set(false);
                    sidebarTgl.set(false);
                    backdropTgl.set(false);
                    activity_bar_tgl.set(false);
                }
            }
        }

        // Desktop equivalent of the mobile backdrop above, but invisible —
        // just asked for "click anywhere else and it closes," not a dimmed
        // screen. Sits behind the activity bar panel (z-59 vs. its z-[60]),
        // so a click that actually lands on the panel never reaches this.
        if *activity_bar_tgl.read() {
            div {
                id: "activity-bar-backdrop",
                class: if *is_desktop_viewport.read() { "fixed inset-0 z-[59]" } else { "hidden" },
                onclick: move |_| {
                    backdropTgl.set(false);
                    activity_bar_tgl.set(false);
                }
            }
        }

        div {
            style: "background-color:{BG};",
            FullScreenEditorModalCmp { }
            OnTheFlyCmp { }
            button {
                id: "add-moment-button",
                // A wide, touch-primary viewport (iPad) still needs this —
                // there's no hover state on touch to reveal some other way
                // in, so `is_desktop_viewport` alone (window size only)
                // wrongly hid it there. See AppState::is_touch_device.
                class: if *is_desktop_viewport.read() && !*is_touch_device.read() { "hidden" } else { "fixed h-14 w-14 bottom-6 right-6 z-51 rounded-full shadow-lg flex items-center justify-center text-2xl font-semibold text-white transition-transform duration-200 hover:scale-105 active:scale-95" },
                style: "background-color:{HL};",
                onclick: move |_| {
                    let current = *momentInputTgl.read();
                    momentInputTgl.set(!current);
                },
                if *momentInputTgl.read() { "✕" } else { "+" }
            }
            div {
                class: {
                    let hidden_on_desktop = if *is_desktop_viewport.read() && !*is_touch_device.read() { "hidden " } else { "" };
                    if *momentInputTgl.read() {
                        format!("{hidden_on_desktop}fixed inset-x-0 bottom-24 z-50 transition-all duration-200 opacity-100 translate-y-0")
                    } else {
                        format!("{hidden_on_desktop}fixed inset-x-0 bottom-24 z-50 transition-all duration-200 opacity-0 translate-y-4 pointer-events-none")
                    }
                },
                MomentInputCmp { }
            }
            Sidebar { }
            div {
                // No overflow-hidden here — #activity-bar below is
                // position: fixed, and iOS Safari has long-standing bugs
                // where an overflow:hidden ancestor clips/breaks a fixed
                // descendant's own internal scrolling even though it
                // shouldn't per spec (fixed elements are supposed to
                // escape ancestor overflow entirely). Every child here
                // (sidebar, main content, activity-bar) already constrains
                // and scrolls itself independently, so this was never load
                // -bearing for containing page-level scroll.
                style: "background-color:{BG};",
                class: "flex h-screen w-full",
                div {
                    // No transform/translate here — this desktop sidebar
                    // never animates (it's not the mobile drawer, which
                    // does need translate-x-0/-translate-x-full to slide,
                    // see Sidebar's own component above). A stray
                    // translate-x-0 copy-pasted from that mobile version
                    // was silently establishing a containing block for
                    // every position:fixed descendant (see this sidebar's
                    // confirmation modals in entity_list_cmp/tag_list_cmp/
                    // project_list_cmp), pinning them inside the sidebar's
                    // 256px box instead of centering on the real viewport.
                    class: if *is_desktop_viewport.read() && !*sidebar_collapsed.read() { "block h-full overflow-y-auto pb-[200px] w-64 border-r border-border bg-background" } else { "hidden" },
                    div {
                        class:"h-1",
                    }
                    vault_switcher_cmp { }
                    div {
                        class: "px-3 mt-1 pt-4 border-t border-border",
                        span {
                            class: "block px-3 mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground",
                            "Views"
                        }
                        views_list_cmp { }
                    }
                    entity_list_cmp { }
                    project_list_cmp { }
                    tag_list_cmp { }
                }
                div {
                    // pb-[200px] on mobile only — same reason as the
                    // sidebar/activity-bar panels (a fixed floating button
                    // sitting over the last bit of scrolled content), but
                    // not wanted on desktop where nothing floats over this
                    // column.
                    class: if *is_desktop_viewport.read() && !*sidebar_collapsed.read() { "w-2/3 overflow-y-auto [&::-webkit-scrollbar]:w-1 [&::-webkit-scrollbar-track]:bg-transparent [&::-webkit-scrollbar-thumb]:bg-black/30" } else { "w-full overflow-y-auto pb-[200px] [&::-webkit-scrollbar]:w-1 [&::-webkit-scrollbar-track]:bg-transparent [&::-webkit-scrollbar-thumb]:bg-black/30" },
                    "{header_title}"
                    Outlet::<Route> {}
                }
                div {
                    id:"activity-bar",
                    class: "{activity_bar_class} bg-background border-l border-border shadow-2xl",
                    if current_moment.read().is_some()
                        || *activity_bar_view.read() == ABView::Story
                        || *activity_bar_view.read() == ABView::Stats
                        || *activity_bar_view.read() == ABView::Info
                        || *activity_bar_view.read() == ABView::Momentos {
                        match activity_bar_view.read().clone() {
                            ABView::Task => rsx! {
                                ab_task_cmp {
                                    key: "{*activity_bar_tgl.read()}"
                                }
                            },
                            ABView::Story => rsx! {
                                ab_story_cmp {
                                    key: "{*activity_bar_tgl.read()}"
                                }
                            },
                            ABView::Stats => rsx! {
                                ab_stats_cmp {
                                    key: "{*activity_bar_tgl.read()}"
                                }
                            },
                            ABView::Info => rsx! {
                                ab_info_cmp {
                                    key: "{*activity_bar_tgl.read()}"
                                }
                            },
                            ABView::Momentos => rsx! {
                                ab_momentos_cmp {
                                    key: "{*activity_bar_tgl.read()}"
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod sync_flush_tests {
    use super::*;

    #[test]
    fn newer_server_timestamp_is_stale() {
        assert!(is_stale("2026-01-01T00:00:00Z", "2026-01-02T00:00:00Z"));
    }

    #[test]
    fn older_or_equal_server_timestamp_is_not_stale() {
        assert!(!is_stale("2026-01-02T00:00:00Z", "2026-01-01T00:00:00Z"));
        assert!(!is_stale("2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z"));
    }

    #[test]
    fn missing_or_unparseable_timestamps_fail_open_not_stale() {
        assert!(!is_stale("", "2026-01-02T00:00:00Z"));
        assert!(!is_stale("2026-01-01T00:00:00Z", ""));
        assert!(!is_stale("garbage", "2026-01-02T00:00:00Z"));
        assert!(!is_stale("2026-01-01T00:00:00Z", "garbage"));
    }

    #[test]
    fn remap_id_substitutes_a_known_temp_id() {
        let mut map = HashMap::new();
        map.insert("temp-1".to_string(), "real-42".to_string());
        assert_eq!(remap_id("temp-1", &map), "real-42");
        assert_eq!(remap_id("unrelated", &map), "unrelated");
    }

    #[test]
    fn remap_value_only_substitutes_matching_string_values() {
        let mut map = HashMap::new();
        map.insert("temp-1".to_string(), "real-42".to_string());
        assert_eq!(remap_value(serde_json::json!("temp-1"), &map), serde_json::json!("real-42"));
        assert_eq!(remap_value(serde_json::json!("other"), &map), serde_json::json!("other"));
        assert_eq!(remap_value(serde_json::json!(5), &map), serde_json::json!(5));
    }

    // The actual 2026-08-07 bug: a moment's queued "metadata" field update is
    // a whole object with a temp entity id buried inside
    // additional_entity_ids (a nested array) — a co-created second @mention
    // that hadn't resolved to its real server id yet at push time. This has
    // to come back remapped just as reliably as a bare top-level id would.
    #[test]
    fn remap_value_recurses_into_nested_arrays_and_objects() {
        let mut map = HashMap::new();
        map.insert("temp-entity-1".to_string(), "real-entity-9".to_string());
        map.insert("temp-moment-1".to_string(), "real-moment-3".to_string());

        let metadata = serde_json::json!({
            "tags": ["book-club"],
            "additional_entity_ids": ["temp-entity-1", "unrelated-entity"],
            "depends_on": ["temp-moment-1"],
        });
        let remapped = remap_value(metadata, &map);
        assert_eq!(
            remapped,
            serde_json::json!({
                "tags": ["book-club"],
                "additional_entity_ids": ["real-entity-9", "unrelated-entity"],
                "depends_on": ["real-moment-3"],
            })
        );
    }

    #[test]
    fn no_prior_refresh_is_always_due() {
        assert!(is_refresh_due(None, 1_000_000));
    }

    #[test]
    fn recent_refresh_is_not_due_yet() {
        let now = 1_000_000;
        assert!(!is_refresh_due(Some(now - 60), now));
    }

    #[test]
    fn refresh_older_than_the_gate_is_due_again() {
        let now = 1_000_000;
        assert!(is_refresh_due(Some(now - MIN_REFRESH_INTERVAL_SECS - 1), now));
    }

    // The exact scenario this whole gate exists for: the 60-second flush
    // loop and the 50-minute proactive loop both independently wanting to
    // refresh — without this gate, both would call refresh_access_token,
    // and since Supabase rotates the refresh token on use, whichever
    // request lands second would be spending an already-consumed token.
    #[test]
    fn a_refresh_moments_ago_blocks_a_second_refresh_right_after() {
        let now = 1_000_000;
        assert!(!is_refresh_due(Some(now - 1), now));
    }
}
