// Offline-first sync for the Synced vault — the read-side cache half (see
// sync_queue.rs for the write-side queue). A flat, localStorage-backed copy
// of the current Synced vault's moments/entities/entity_types, so
// SupabaseStorage's reads (storage.rs) can return instantly instead of
// always waiting on the network, and so a reload while offline shows the
// vault as it last was instead of a blank screen.
//
// Deliberately NOT shaped like local.rs's per-entity YAML-frontmatter
// files — that shape exists there to match the desktop file-per-entity
// vault format. Synced already deals in two flat lists over the wire
// (exactly what getMoments/getEntities return), so the mirror is just
// those same lists, JSON-encoded, one key each. Moments are stored
// including soft-deleted ones (deleted_at set) — get_moments/
// get_deleted_moments in storage.rs filter this one list client-side,
// mirroring the server's own `deleted_at=is.null` / `not.is.null` query
// split, so there's no separate trash log to build.
//
// Keyed under `peeplist_synced_cache:*`, distinct from Local vault's own
// `peeplist_vault:*` keys (local.rs) — the two must never collide, since a
// browser profile can have both a Local vault and a logged-in Synced
// account at once.
//
// Encode/decode are kept pure (no web_sys) so they're unit-testable the
// same way vault_format.rs is; only the get_*/set_*/clear wrappers touch
// the browser.

use crate::types::*;
use serde_json::Value;
use web_sys::Storage;

const MOMENTS_KEY: &str = "peeplist_synced_cache:moments";
const ENTITIES_KEY: &str = "peeplist_synced_cache:entities";
const ENTITY_TYPES_KEY: &str = "peeplist_synced_cache:entity_types";

fn local_storage() -> Option<Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

// MomentType/EntityType's `created_at` is `#[serde(skip_serializing,
// default)]` — right for the HTTP boundary (never send a client-side
// timestamp back over a value the server assigns), but it means a plain
// `serde_json::to_value` silently drops it, and decoding back would leave
// every mirrored record's created_at blank. Same gotcha, same fix as
// local.rs's `to_patchable_value`: reinsert it by hand before encoding.
fn moment_to_value(m: &MomentType) -> Value {
    let mut v = serde_json::to_value(m).expect("MomentType always serializes");
    if let Some(obj) = v.as_object_mut() {
        obj.insert("created_at".to_string(), Value::String(m.created_at.clone()));
        // updated_at is skip_serializing too, and unlike created_at, the
        // LWW conflict check (see layouts/navbar.rs's flush loop) actually
        // depends on reading a real value back out of the mirror — losing
        // it here would silently make every mirrored moment look
        // "never updated," which is exactly the state that always loses
        // an LWW comparison to whatever the server has.
        obj.insert("updated_at".to_string(), Value::String(m.updated_at.clone()));
    }
    v
}

fn entity_to_value(e: &EntityType) -> Value {
    let mut v = serde_json::to_value(e).expect("EntityType always serializes");
    if let Some(obj) = v.as_object_mut() {
        obj.insert("created_at".to_string(), Value::String(e.created_at.clone()));
        obj.insert("updated_at".to_string(), Value::String(e.updated_at.clone()));
    }
    v
}

fn encode_moments(moments: &[MomentType]) -> String {
    let values: Vec<Value> = moments.iter().map(moment_to_value).collect();
    serde_json::to_string(&values).unwrap_or_else(|_| "[]".to_string())
}

fn decode_moments(raw: &str) -> Option<Vec<MomentType>> {
    let values: Vec<Value> = serde_json::from_str(raw).ok()?;
    values.into_iter().map(|v| serde_json::from_value(v).ok()).collect()
}

fn encode_entities(entities: &[EntityType]) -> String {
    let values: Vec<Value> = entities.iter().map(entity_to_value).collect();
    serde_json::to_string(&values).unwrap_or_else(|_| "[]".to_string())
}

fn decode_entities(raw: &str) -> Option<Vec<EntityType>> {
    let values: Vec<Value> = serde_json::from_str(raw).ok()?;
    values.into_iter().map(|v| serde_json::from_value(v).ok()).collect()
}

// Applies a single-field patch the same way local.rs's update_moment_field/
// update_entity_field do (round-trip through a mutable JSON Value so any
// field can be patched generically) — used by both SupabaseStorage's
// optimistic writes (storage.rs) and the flush loop's queue replay
// (layouts/navbar.rs), so the mirror and the eventual server state stay
// shaped the same way. Returns `None` only if the patched Value no longer
// deserializes as a MomentType/EntityType at all (a field name that isn't
// really one of its properties) — callers treat that as "patch had no
// effect" rather than an error, matching a no-op field patch server-side.
pub fn patch_moment(moment: &MomentType, field: &str, value: Value) -> Option<MomentType> {
    let mut json = moment_to_value(moment);
    if let Some(obj) = json.as_object_mut() {
        obj.insert(field.to_string(), value);
    }
    serde_json::from_value(json).ok()
}

pub fn patch_entity(entity: &EntityType, field: &str, value: Value) -> Option<EntityType> {
    let mut json = entity_to_value(entity);
    if let Some(obj) = json.as_object_mut() {
        obj.insert(field.to_string(), value);
    }
    serde_json::from_value(json).ok()
}

// `None` means "never seeded" (cold cache — the caller should hit the
// network and seed it), as opposed to `Some(vec![])`, a real empty vault.
pub fn get_moments() -> Option<Vec<MomentType>> {
    let storage = local_storage()?;
    let raw = storage.get_item(MOMENTS_KEY).ok().flatten()?;
    decode_moments(&raw)
}

pub fn set_moments(moments: &[MomentType]) {
    if let Some(storage) = local_storage() {
        let _ = storage.set_item(MOMENTS_KEY, &encode_moments(moments));
    }
}

pub fn get_entities() -> Option<Vec<EntityType>> {
    let storage = local_storage()?;
    let raw = storage.get_item(ENTITIES_KEY).ok().flatten()?;
    decode_entities(&raw)
}

pub fn set_entities(entities: &[EntityType]) {
    if let Some(storage) = local_storage() {
        let _ = storage.set_item(ENTITIES_KEY, &encode_entities(entities));
    }
}

pub fn get_entity_types() -> Option<Vec<EntityTypeType>> {
    let storage = local_storage()?;
    let raw = storage.get_item(ENTITY_TYPES_KEY).ok().flatten()?;
    serde_json::from_str(&raw).ok()
}

pub fn set_entity_types(types: &[EntityTypeType]) {
    if let Some(storage) = local_storage() {
        if let Ok(raw) = serde_json::to_string(types) {
            let _ = storage.set_item(ENTITY_TYPES_KEY, &raw);
        }
    }
}

// Wipes every cached key — called on logout / "delete my account" (see
// components/settings.rs) so a stale mirror never survives past the
// session it belongs to.
pub fn clear() {
    if let Some(storage) = local_storage() {
        let _ = storage.remove_item(MOMENTS_KEY);
        let _ = storage.remove_item(ENTITIES_KEY);
        let _ = storage.remove_item(ENTITY_TYPES_KEY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_moment(id: &str, created_at: &str) -> MomentType {
        MomentType {
            id: id.to_string(),
            title: "Call Mom".to_string(),
            description: None,
            gravity: Some(1),
            entity_id: "e1".to_string(),
            moment_type_id: 1,
            due_at: None,
            completed_at: None,
            deleted_at: None,
            reactions: None,
            created_at: created_at.to_string(),
            // Deliberately distinct from created_at in these fixtures, so a
            // round-trip test can't pass by accident just because the two
            // happen to share a value.
            updated_at: "2026-03-15T12:00:00Z".to_string(),
            depends_on: None,
            metadata: None,
        }
    }

    fn sample_entity(id: &str, created_at: &str) -> EntityType {
        EntityType {
            id: id.to_string(),
            name: "Alex".to_string(),
            entity_type_id: None,
            parent_entity_id: None,
            created_at: created_at.to_string(),
            updated_at: "2026-03-15T12:00:00Z".to_string(),
            drift: 2.0,
            metadata: None,
        }
    }

    #[test]
    fn moment_round_trip_preserves_created_at_despite_skip_serializing() {
        let moments = vec![sample_moment("m1", "2026-01-01T00:00:00Z")];
        let raw = encode_moments(&moments);
        let decoded = decode_moments(&raw).expect("valid json decodes");
        assert_eq!(decoded, moments);
        assert_eq!(decoded[0].created_at, "2026-01-01T00:00:00Z");
    }

    #[test]
    fn entity_round_trip_preserves_created_at_despite_skip_serializing() {
        let entities = vec![sample_entity("e1", "2026-01-01T00:00:00Z")];
        let raw = encode_entities(&entities);
        let decoded = decode_entities(&raw).expect("valid json decodes");
        assert_eq!(decoded, entities);
        assert_eq!(decoded[0].created_at, "2026-01-01T00:00:00Z");
    }

    // updated_at is what the LWW conflict check in the flush loop
    // (layouts/navbar.rs) actually reads out of the mirror — if this
    // round-trip silently dropped it (the same skip_serializing gotcha
    // created_at has), every mirrored record would look "never updated"
    // and always lose a real conflict comparison.
    #[test]
    fn moment_round_trip_preserves_updated_at_despite_skip_serializing() {
        let moments = vec![sample_moment("m1", "2026-01-01T00:00:00Z")];
        let raw = encode_moments(&moments);
        let decoded = decode_moments(&raw).expect("valid json decodes");
        assert_eq!(decoded[0].updated_at, "2026-03-15T12:00:00Z");
    }

    #[test]
    fn entity_round_trip_preserves_updated_at_despite_skip_serializing() {
        let entities = vec![sample_entity("e1", "2026-01-01T00:00:00Z")];
        let raw = encode_entities(&entities);
        let decoded = decode_entities(&raw).expect("valid json decodes");
        assert_eq!(decoded[0].updated_at, "2026-03-15T12:00:00Z");
    }

    #[test]
    fn soft_deleted_moments_round_trip_with_deleted_at_intact() {
        let mut m = sample_moment("m1", "2026-01-01T00:00:00Z");
        m.deleted_at = Some("2026-02-01T00:00:00Z".to_string());
        let raw = encode_moments(&[m.clone()]);
        let decoded = decode_moments(&raw).unwrap();
        assert_eq!(decoded[0].deleted_at, m.deleted_at);
    }

    #[test]
    fn patch_moment_applies_field_and_keeps_created_and_updated_at() {
        let m = sample_moment("m1", "2026-01-01T00:00:00Z");
        let patched = patch_moment(&m, "title", Value::String("Call Dad".to_string())).unwrap();
        assert_eq!(patched.title, "Call Dad");
        assert_eq!(patched.created_at, "2026-01-01T00:00:00Z");
        assert_eq!(patched.updated_at, "2026-03-15T12:00:00Z");
    }

    #[test]
    fn patch_entity_applies_field_and_keeps_created_and_updated_at() {
        let e = sample_entity("e1", "2026-01-01T00:00:00Z");
        let patched = patch_entity(&e, "name", Value::String("Alexandra".to_string())).unwrap();
        assert_eq!(patched.name, "Alexandra");
        assert_eq!(patched.created_at, "2026-01-01T00:00:00Z");
        assert_eq!(patched.updated_at, "2026-03-15T12:00:00Z");
    }

    #[test]
    fn decode_of_garbage_is_none_not_a_panic() {
        assert_eq!(decode_moments("not json"), None);
        assert_eq!(decode_entities("not json"), None);
    }

    #[test]
    fn decode_of_empty_array_is_some_empty_vec() {
        assert_eq!(decode_moments("[]"), Some(Vec::new()));
        assert_eq!(decode_entities("[]"), Some(Vec::new()));
    }
}
