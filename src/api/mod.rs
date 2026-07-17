pub mod client;
pub mod entity;
pub mod moment;
pub mod auth;
pub mod storage;
pub mod vault_format;
pub mod import;
// Two concrete Local vault backends, sharing vault_format — `dx build`
// compiles in `desktop` regardless of whether default features (which
// include `web`) are also active, so this has to key off `desktop` being
// *present*, not `web` being absent. See storage.rs's `pub use` below,
// which picks whichever of these is actually compiled in as `LocalStorage`.
#[cfg(feature = "desktop")]
pub mod local_desktop;
#[cfg(not(feature = "desktop"))]
pub mod local;

pub use auth::{login, signup, SignupOutcome, get_current_user, refresh_access_token};
pub use storage::{ActiveStorage, VaultKind, StorageError, is_self_entity};
pub use import::{import_local_into_synced, ImportSummary};

// The `entities`/`moments` tables still have `bigint` FK columns
// (entity_id, depends_on, entity_type_id, parent_entity_id) even though the
// app's ids are now Strings app-wide (see types.rs's de_flex_id/se_flex_id).
// update_moment_field/update_entity_field build their PATCH payload from a
// raw serde_json::Value passed in by call sites, so — unlike the typed
// struct fields, which have serialize_with — this is the one spot that has
// to coerce a stringified FK id back into a JSON number before it hits a
// still-bigint column.
pub(crate) fn coerce_fk_value(field: &str, value: serde_json::Value) -> serde_json::Value {
    const FK_FIELDS: &[&str] = &["entity_id", "depends_on", "entity_type_id", "parent_entity_id"];
    if !FK_FIELDS.contains(&field) {
        return value;
    }
    match value {
        serde_json::Value::String(s) => s.parse::<i64>()
            .map(serde_json::Value::from)
            .unwrap_or(serde_json::Value::String(s)),
        other => other,
    }
}
