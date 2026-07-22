use serde_json::Value;
use crate::types::*;

// Local-first pivot, Phase 1b (see /Users/aogposton/.claude/plans/joyful-brewing-feather.md
// and memory reference_local_first_pivot_plan). Backend selection becomes a
// user-chosen vault instead of just "is there a token" — this module is the
// seam every call site goes through instead of calling api::entity::*/
// api::moment::* directly.
//
// Dispatch is enum-based, not `dyn Trait` + async_trait: async_trait's `Send`
// bound breaks under wasm (Dioxus web futures are `!Send`), and there are
// only ever two concrete backends. Every method below is `async fn`, even
// LocalStorage's (which has no real `.await` inside yet) — this costs
// nothing and means zero call-site restructuring for sync-vs-async, since
// every call site already wraps calls in `spawn(async move { ... .await })`.

#[derive(Debug)]
pub enum StorageError {
    Network(reqwest::Error),
    // Kept for a future backend/path that's genuinely not implemented yet
    // (e.g. the desktop filesystem backend — see memory
    // reference_local_first_pivot_plan). The web LocalStorage backend
    // (src/api/local.rs) is real as of Phase 1e's web slice and uses
    // StorageError::Local for its own failures instead.
    NotImplemented,
    // LocalStorage-specific failures: localStorage unavailable, a record
    // referenced by id that isn't in the vault, or a JSON merge-patch that
    // didn't round-trip. See src/api/local.rs.
    Local(String),
    // A Supabase request that reached the server and got a real response
    // back (unlike Network, which is transport-level failure) — the server
    // just rejected it. Distinct from Local: this is the Synced vault's
    // backend saying no (e.g. a foreign-key violation on entity delete),
    // not a client-side storage problem.
    Remote(String),
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageError::Network(e) => write!(f, "{e}"),
            StorageError::NotImplemented => write!(f, "not implemented yet"),
            StorageError::Local(msg) => write!(f, "{msg}"),
            StorageError::Remote(msg) => write!(f, "{msg}"),
        }
    }
}

impl From<reqwest::Error> for StorageError {
    fn from(e: reqwest::Error) -> Self {
        StorageError::Network(e)
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum VaultKind {
    Local,
    Synced,
}

impl VaultKind {
    pub fn as_storage_str(&self) -> &'static str {
        match self {
            VaultKind::Local => "local",
            VaultKind::Synced => "synced",
        }
    }

    pub fn from_storage_str(s: &str) -> VaultKind {
        match s {
            "local" => VaultKind::Local,
            _ => VaultKind::Synced,
        }
    }

    // `active_vault` defaults to Synced app-wide (see AppState in main.rs —
    // deliberate, so an already-logged-in session doesn't regress to an
    // empty Local vault the moment this existed). That means the raw signal
    // can say Synced on a session that was never logged in at all, or that
    // just logged out. `effective` collapses that down to what's actually
    // going to happen: Synced only holds if there's really a token,
    // otherwise Local. `ActiveStorage::for_vault` already applies exactly
    // this rule when picking a backend — anything that reads `active_vault`
    // for a *display or logic* decision (not just to construct storage)
    // needs to go through this too, or it'll disagree with what the
    // storage layer actually does. Getting this wrong is what caused two
    // real bugs already: the vault switcher showing the wrong vault as
    // current, and MomentInputCmp defaulting new moments to the Supabase
    // self id ("0") even while actually writing to the Local vault.
    pub fn effective(self, token: &Option<String>) -> VaultKind {
        match self {
            VaultKind::Synced if token.is_some() => VaultKind::Synced,
            _ => VaultKind::Local,
        }
    }

    // The "this entity means yourself" id differs per vault, and for Synced
    // it's no longer a fixed sentinel at all (see types.rs's
    // SELF_ENTITY_TYPE_ID doc comment) — every account has its own Self
    // entity row now, found by scanning the already-fetched entities list
    // rather than assumed by a hardcoded id. Local still has a real fixed
    // sentinel (each local vault mints its own self.md at that id, so no
    // lookup needed). None only if a Synced account somehow has no Self
    // entity yet — shouldn't happen once the signup trigger/backfill in
    // scripts/2026-07-22-rls-and-self-entity.sql has run, but callers still
    // have to handle it since entities may not have loaded yet either.
    pub fn resolve_self_entity_id(&self, entities: &[crate::types::EntityType]) -> Option<String> {
        match self {
            VaultKind::Local => Some(super::vault_format::LOCAL_SELF_ENTITY_ID.to_string()),
            VaultKind::Synced => entities.iter().find(|e| is_self_entity(e)).map(|e| e.id.clone()),
        }
    }
}

// A local vault's self entity keeps a real fixed sentinel id (safe to
// compare directly — see vault_format::LOCAL_SELF_ENTITY_ID). A Synced
// self entity no longer has one (see types.rs's SELF_ENTITY_TYPE_ID doc
// comment): it's identified by entity_type_id instead, which is why this
// takes the whole entity rather than just an id string now.
pub fn is_self_entity(entity: &crate::types::EntityType) -> bool {
    entity.id == super::vault_format::LOCAL_SELF_ENTITY_ID
        || entity.entity_type_id.as_deref() == Some(crate::types::SELF_ENTITY_TYPE_ID)
}

pub struct SupabaseStorage {
    token: String,
}

impl SupabaseStorage {
    pub fn new(token: String) -> Self {
        Self { token }
    }

    pub async fn get_moments(&self) -> Result<Vec<MomentType>, StorageError> {
        Ok(super::moment::getMoments(self.token.clone()).await?)
    }

    pub async fn get_entities(&self) -> Result<Vec<EntityType>, StorageError> {
        Ok(super::entity::getEntities(self.token.clone()).await?)
    }

    pub async fn get_entity_types(&self) -> Result<Vec<EntityTypeType>, StorageError> {
        Ok(super::entity::getEntityTypes(self.token.clone()).await?)
    }

    pub async fn create_moment(&self, m: NewMomentType) -> Result<MomentType, StorageError> {
        Ok(super::moment::createMoment(m, self.token.clone()).await?)
    }

    pub async fn create_entity(&self, e: NewEntityType) -> Result<EntityType, StorageError> {
        Ok(super::entity::createEntity(e, self.token.clone()).await?)
    }

    pub async fn create_reaction(&self, r: NewReactionType) -> Result<ReactionType, StorageError> {
        Ok(super::moment::createReaction(r, self.token.clone()).await?)
    }

    pub async fn update_moment_field(&self, id: String, field: &str, value: Value) -> Result<(), StorageError> {
        Ok(super::moment::update_moment_field(id, field, value, self.token.clone()).await?)
    }

    pub async fn update_entity_field(&self, id: String, field: &str, value: Value) -> Result<(), StorageError> {
        Ok(super::entity::update_entity_field(id, field, value, self.token.clone()).await?)
    }

    // Unlike Local's storage (see LocalStorage::reassign_moment_entity —
    // moments there live nested inside their entity's own file, so
    // reassignment means physically moving them), Supabase's `moments`
    // table has a real entity_id column — the existing generic field-patch
    // path already handles it correctly (entity_id is in coerce_fk_value's
    // FK_FIELDS list). This just gives it the same method name/shape as
    // the Local backend so ActiveStorage's dispatch below doesn't need to
    // know which backend it's talking to.
    pub async fn reassign_moment_entity(&self, moment_id: String, new_entity_id: String) -> Result<(), StorageError> {
        self.update_moment_field(moment_id, "entity_id", serde_json::json!(new_entity_id)).await
    }

    pub async fn delete_moment(&self, moment: MomentType) -> Result<(), StorageError> {
        Ok(super::moment::deleteMoment(moment, self.token.clone()).await?)
    }

    pub async fn delete_entity(&self, id: String) -> Result<(), StorageError> {
        super::entity::deleteEntity(id, self.token.clone()).await.map_err(StorageError::Remote)
    }

    pub async fn delete_reaction(&self, reaction: ReactionType) -> Result<(), StorageError> {
        Ok(super::moment::deleteReaction(reaction, self.token.clone()).await?)
    }

    pub async fn get_deleted_moments(&self) -> Result<Vec<MomentType>, StorageError> {
        Ok(super::moment::getDeletedMoments(self.token.clone()).await?)
    }

    pub async fn restore_moment(&self, id: String) -> Result<(), StorageError> {
        Ok(super::moment::restoreMoment(id, self.token.clone()).await?)
    }
}

// Two concrete implementations, chosen at compile time (see mod.rs) — a
// browser-localStorage-backed one for web, a real std::fs one for desktop,
// both flat-file-shaped via vault_format. Re-exported here under one name
// so ActiveStorage's match arms below read the same either way as
// SupabaseStorage.
#[cfg(feature = "desktop")]
pub use super::local_desktop::LocalStorage;
#[cfg(not(feature = "desktop"))]
pub use super::local::LocalStorage;

pub enum ActiveStorage {
    Local(LocalStorage),
    Supabase(SupabaseStorage),
}

impl ActiveStorage {
    pub fn for_vault(vault: VaultKind, token: Option<String>) -> Self {
        match vault.effective(&token) {
            // effective() only returns Synced when token is Some, so the
            // token.expect() below can't actually fire.
            VaultKind::Synced => ActiveStorage::Supabase(SupabaseStorage::new(token.expect("effective() guarantees a token here"))),
            VaultKind::Local => ActiveStorage::Local(LocalStorage::new()),
        }
    }

    pub async fn get_moments(&self) -> Result<Vec<MomentType>, StorageError> {
        match self {
            ActiveStorage::Local(l) => l.get_moments().await,
            ActiveStorage::Supabase(s) => s.get_moments().await,
        }
    }

    pub async fn get_entities(&self) -> Result<Vec<EntityType>, StorageError> {
        match self {
            ActiveStorage::Local(l) => l.get_entities().await,
            ActiveStorage::Supabase(s) => s.get_entities().await,
        }
    }

    pub async fn get_entity_types(&self) -> Result<Vec<EntityTypeType>, StorageError> {
        match self {
            ActiveStorage::Local(l) => l.get_entity_types().await,
            ActiveStorage::Supabase(s) => s.get_entity_types().await,
        }
    }

    pub async fn create_moment(&self, m: NewMomentType) -> Result<MomentType, StorageError> {
        match self {
            ActiveStorage::Local(l) => l.create_moment(m).await,
            ActiveStorage::Supabase(s) => s.create_moment(m).await,
        }
    }

    pub async fn create_entity(&self, e: NewEntityType) -> Result<EntityType, StorageError> {
        match self {
            ActiveStorage::Local(l) => l.create_entity(e).await,
            ActiveStorage::Supabase(s) => s.create_entity(e).await,
        }
    }

    pub async fn create_reaction(&self, r: NewReactionType) -> Result<ReactionType, StorageError> {
        match self {
            ActiveStorage::Local(l) => l.create_reaction(r).await,
            ActiveStorage::Supabase(s) => s.create_reaction(r).await,
        }
    }

    pub async fn update_moment_field(&self, id: String, field: &str, value: Value) -> Result<(), StorageError> {
        match self {
            ActiveStorage::Local(l) => l.update_moment_field(id, field, value).await,
            ActiveStorage::Supabase(s) => s.update_moment_field(id, field, value).await,
        }
    }

    pub async fn update_entity_field(&self, id: String, field: &str, value: Value) -> Result<(), StorageError> {
        match self {
            ActiveStorage::Local(l) => l.update_entity_field(id, field, value).await,
            ActiveStorage::Supabase(s) => s.update_entity_field(id, field, value).await,
        }
    }

    pub async fn reassign_moment_entity(&self, moment_id: String, new_entity_id: String) -> Result<(), StorageError> {
        match self {
            ActiveStorage::Local(l) => l.reassign_moment_entity(moment_id, new_entity_id).await,
            ActiveStorage::Supabase(s) => s.reassign_moment_entity(moment_id, new_entity_id).await,
        }
    }

    pub async fn delete_moment(&self, moment: MomentType) -> Result<(), StorageError> {
        match self {
            ActiveStorage::Local(l) => l.delete_moment(moment).await,
            ActiveStorage::Supabase(s) => s.delete_moment(moment).await,
        }
    }

    pub async fn delete_entity(&self, id: String) -> Result<(), StorageError> {
        match self {
            ActiveStorage::Local(l) => l.delete_entity(id).await,
            ActiveStorage::Supabase(s) => s.delete_entity(id).await,
        }
    }

    pub async fn delete_reaction(&self, reaction: ReactionType) -> Result<(), StorageError> {
        match self {
            ActiveStorage::Local(l) => l.delete_reaction(reaction).await,
            ActiveStorage::Supabase(s) => s.delete_reaction(reaction).await,
        }
    }

    pub async fn get_deleted_moments(&self) -> Result<Vec<MomentType>, StorageError> {
        match self {
            ActiveStorage::Local(l) => l.get_deleted_moments().await,
            ActiveStorage::Supabase(s) => s.get_deleted_moments().await,
        }
    }

    pub async fn restore_moment(&self, id: String) -> Result<(), StorageError> {
        match self {
            ActiveStorage::Local(l) => l.restore_moment(id).await,
            ActiveStorage::Supabase(s) => s.restore_moment(id).await,
        }
    }
}
