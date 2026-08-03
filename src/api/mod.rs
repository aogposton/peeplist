pub mod client;
pub mod entity;
pub mod moment;
pub mod auth;
pub mod storage;
pub mod vault_format;
pub mod import;
pub mod error_report;
// Offline-first sync for the Synced vault: sync_queue.rs persists pending
// writes, synced_mirror.rs caches reads. Both are plain localStorage-backed
// modules used by SupabaseStorage (storage.rs) and the flush loop
// (layouts/navbar.rs) — see synced_mirror.rs's own header comment.
pub mod sync_queue;
pub mod synced_mirror;
// Two concrete Local vault backends, sharing vault_format — `dx build`
// compiles in `desktop` regardless of whether default features (which
// include `web`) are also active, so this has to key off `native` being
// *present*, not `web` being absent. `native` is shared by both the
// desktop GUI (`desktop` feature) and the standalone `bsb` CLI binary, so
// either one gets the real std::fs-backed vault. See storage.rs's `pub
// use` below, which picks whichever of these is actually compiled in as
// `LocalStorage`.
#[cfg(feature = "native")]
pub mod local_desktop;
#[cfg(not(feature = "native"))]
pub mod local;

pub use auth::{login, signup, SignupOutcome, get_current_user, refresh_access_token, update_password, request_password_reset, AuthError};
pub use storage::{ActiveStorage, VaultKind, StorageError, is_self_entity};
pub use import::{import_local_into_synced, export_backup, import_backup, ImportSummary};
pub use error_report::report_error;

// Desktop/CLI already write real files under ~/Documents/Peeplist — this is
// specifically the web build's missing "get my data back out" path, since
// a web Local vault only ever exists inside localStorage otherwise.
#[cfg(not(feature = "native"))]
pub async fn export_local_vault() -> Result<Vec<(String, String)>, StorageError> {
    local::LocalStorage::new().export_all().await
}

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
