use serde_json::Value;
use crate::types::*;
use super::client::SupabaseClient;
use super::sync_queue::{self, QueuedOp};
use super::synced_mirror;
use uuid::Uuid;

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

    // getMoments/getDeletedMoments are two independently-filtered views of
    // the same server table (deleted_at is.null / not.is.null) — a cold
    // mirror has to seed from *both* together, or whichever of get_moments/
    // get_deleted_moments happens to run first would seed the mirror with
    // only its own half and starve the other view forever after (it'd never
    // see an empty mirror again to trigger a re-seed).
    async fn ensure_moments_seeded(&self) -> Result<Vec<MomentType>, StorageError> {
        if let Some(cached) = synced_mirror::get_moments() {
            return Ok(cached);
        }
        let mut all = super::moment::getMoments(self.token.clone()).await?;
        let deleted = super::moment::getDeletedMoments(self.token.clone()).await?;
        all.extend(deleted);
        synced_mirror::set_moments(&all);
        Ok(all)
    }

    // Cache-first (offline-first sync, see api::synced_mirror/sync_queue):
    // a populated mirror returns instantly with no network round-trip at
    // all; only a cold mirror (first login, or a cleared cache) falls back
    // to the direct fetch, seeding the mirror from the result. No call site
    // changes needed — this stays behind the same `ActiveStorage` seam
    // every one of them already goes through.
    pub async fn get_moments(&self) -> Result<Vec<MomentType>, StorageError> {
        Ok(self.ensure_moments_seeded().await?.into_iter().filter(|m| m.deleted_at.is_none()).collect())
    }

    pub async fn get_entities(&self) -> Result<Vec<EntityType>, StorageError> {
        if let Some(cached) = synced_mirror::get_entities() {
            return Ok(cached);
        }
        let fresh = super::entity::getEntities(self.token.clone()).await?;
        synced_mirror::set_entities(&fresh);
        Ok(fresh)
    }

    pub async fn get_entity_types(&self) -> Result<Vec<EntityTypeType>, StorageError> {
        if let Some(cached) = synced_mirror::get_entity_types() {
            return Ok(cached);
        }
        let fresh = super::entity::getEntityTypes(self.token.clone()).await?;
        synced_mirror::set_entity_types(&fresh);
        Ok(fresh)
    }

    // Writes apply to the mirror immediately (same instant-Signal-update UX
    // Local vault already has — see views/home.rs, which just does
    // `moments.write().push(created)` the moment this future resolves) and
    // queue the real network call for the background flush loop
    // (layouts/navbar.rs) instead of awaiting it here. A client-minted
    // `temp_id` stands in for the server's real id until the flush loop's
    // create actually lands — see sync_queue::QueuedOp's doc comment.
    pub async fn create_moment(&self, m: NewMomentType) -> Result<MomentType, StorageError> {
        let temp_id = Uuid::new_v4().to_string();
        let moment = MomentType {
            id: temp_id.clone(),
            title: m.title.clone(),
            description: m.description.clone(),
            gravity: m.gravity,
            entity_id: m.entity_id.clone(),
            moment_type_id: m.moment_type_id,
            due_at: None,
            completed_at: None,
            deleted_at: m.deleted_at.clone(),
            reactions: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            depends_on: None,
            metadata: None,
        };
        let mut moments = synced_mirror::get_moments().unwrap_or_default();
        moments.push(moment.clone());
        synced_mirror::set_moments(&moments);
        sync_queue::push(QueuedOp::CreateMoment { temp_id, new: m });
        Ok(moment)
    }

    pub async fn create_entity(&self, e: NewEntityType) -> Result<EntityType, StorageError> {
        let temp_id = Uuid::new_v4().to_string();
        let entity = EntityType {
            id: temp_id.clone(),
            name: e.name.clone(),
            entity_type_id: e.entity_type_id.clone(),
            parent_entity_id: e.parent_entity_id.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            drift: 2.0,
            metadata: e.metadata.clone(),
        };
        let mut entities = synced_mirror::get_entities().unwrap_or_default();
        entities.push(entity.clone());
        synced_mirror::set_entities(&entities);
        sync_queue::push(QueuedOp::CreateEntity { temp_id, new: e });
        Ok(entity)
    }

    pub async fn create_reaction(&self, r: NewReactionType) -> Result<ReactionType, StorageError> {
        let temp_id = Uuid::new_v4().to_string();
        let reaction = ReactionType {
            id: temp_id.clone(),
            description: r.description.clone(),
            moment_id: r.moment_id.clone(),
            value: r.value,
        };
        let mut moments = synced_mirror::get_moments().unwrap_or_default();
        if let Some(m) = moments.iter_mut().find(|m| m.id == r.moment_id) {
            m.reactions.get_or_insert_with(Vec::new).push(reaction.clone());
        }
        synced_mirror::set_moments(&moments);
        sync_queue::push(QueuedOp::CreateReaction { temp_id, new: r });
        Ok(reaction)
    }

    pub async fn update_moment_field(&self, id: String, field: &str, value: Value) -> Result<(), StorageError> {
        let coerced = super::coerce_fk_value(field, value.clone());
        let mut moments = synced_mirror::get_moments().unwrap_or_default();
        // Captured BEFORE patching — this is the last server-confirmed
        // updated_at we know of, the baseline the flush loop's LWW check
        // compares against later. Deliberately not bumped to "now" here:
        // that would be a client-clock timestamp compared against the
        // server's own clock, which can drift.
        let staged_updated_at = moments.iter().find(|m| m.id == id).map(|m| m.updated_at.clone()).unwrap_or_default();
        if let Some(pos) = moments.iter().position(|m| m.id == id) {
            if let Some(patched) = synced_mirror::patch_moment(&moments[pos], field, coerced) {
                moments[pos] = patched;
            }
        }
        synced_mirror::set_moments(&moments);
        sync_queue::push(QueuedOp::UpdateMomentField { id, field: field.to_string(), value, staged_updated_at });
        Ok(())
    }

    pub async fn update_entity_field(&self, id: String, field: &str, value: Value) -> Result<(), StorageError> {
        let coerced = super::coerce_fk_value(field, value.clone());
        let mut entities = synced_mirror::get_entities().unwrap_or_default();
        let staged_updated_at = entities.iter().find(|e| e.id == id).map(|e| e.updated_at.clone()).unwrap_or_default();
        if let Some(pos) = entities.iter().position(|e| e.id == id) {
            if let Some(patched) = synced_mirror::patch_entity(&entities[pos], field, coerced) {
                entities[pos] = patched;
            }
        }
        synced_mirror::set_entities(&entities);
        sync_queue::push(QueuedOp::UpdateEntityField { id, field: field.to_string(), value, staged_updated_at });
        Ok(())
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

    // Soft delete, mirrored immediately (same coerce as update_moment_field
    // above uses, since this is really just a deleted_at patch server-side
    // too — see moment::deleteMoment).
    pub async fn delete_moment(&self, moment: MomentType) -> Result<(), StorageError> {
        let deleted_at = chrono::Utc::now().to_rfc3339();
        let mut moments = synced_mirror::get_moments().unwrap_or_default();
        if let Some(pos) = moments.iter().position(|m| m.id == moment.id) {
            moments[pos].deleted_at = Some(deleted_at);
        }
        synced_mirror::set_moments(&moments);
        sync_queue::push(QueuedOp::DeleteMoment(moment));
        Ok(())
    }

    // A real hard delete server-side, which can fail on a live foreign-key
    // violation (something still references this entity) — unlike every
    // other write here, that rejection used to surface synchronously to the
    // caller (see the old direct-network version of this method). Now that
    // it's queued, a rejection only shows up later in the flush loop's log,
    // not in the UI at the moment of deletion — an accepted tradeoff of
    // going optimistic (see the sync plan's "real risks" section).
    pub async fn delete_entity(&self, id: String) -> Result<(), StorageError> {
        let mut entities = synced_mirror::get_entities().unwrap_or_default();
        entities.retain(|e| e.id != id);
        synced_mirror::set_entities(&entities);
        sync_queue::push(QueuedOp::DeleteEntity(id));
        Ok(())
    }

    pub async fn delete_reaction(&self, reaction: ReactionType) -> Result<(), StorageError> {
        let mut moments = synced_mirror::get_moments().unwrap_or_default();
        if let Some(m) = moments.iter_mut().find(|m| m.id == reaction.moment_id) {
            if let Some(reactions) = m.reactions.as_mut() {
                reactions.retain(|r| r.id != reaction.id);
            }
        }
        synced_mirror::set_moments(&moments);
        sync_queue::push(QueuedOp::DeleteReaction(reaction));
        Ok(())
    }

    pub async fn get_deleted_moments(&self) -> Result<Vec<MomentType>, StorageError> {
        Ok(self.ensure_moments_seeded().await?.into_iter().filter(|m| m.deleted_at.is_some()).collect())
    }

    pub async fn restore_moment(&self, id: String) -> Result<(), StorageError> {
        let mut moments = synced_mirror::get_moments().unwrap_or_default();
        if let Some(pos) = moments.iter().position(|m| m.id == id) {
            moments[pos].deleted_at = None;
        }
        synced_mirror::set_moments(&moments);
        sync_queue::push(QueuedOp::RestoreMoment(id));
        Ok(())
    }

    // "Delete my account" (2026-08-02, replacing the earlier data-only
    // "Delete my account and data" — see memory: a user explicitly asked
    // for one full deletion, not a two-tier data-vs-login choice). Calls
    // the delete-account Supabase Edge Function (supabase/functions/delete-
    // account/index.ts, deployed separately via the Supabase CLI — this
    // client-side code can never safely hold the service-role key deleting
    // the actual auth.users row needs). That function deletes this
    // account's reactions/moments/entities *then* the auth.users row
    // itself, in that order deliberately — deleting the auth row first
    // would violate entities.user_id's foreign key while any of this
    // account's entities (e.g. its own Self row) still reference it.
    pub async fn delete_account(&self) -> Result<(), StorageError> {
        let client = SupabaseClient::new(self.token.clone());
        let resp = client.functions_post("delete-account").send().await
            .map_err(|e| StorageError::Remote(e.to_string()))?;
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(StorageError::Remote(format!("Failed to delete account: {text}")));
        }
        Ok(())
    }
}

// Two concrete implementations, chosen at compile time (see mod.rs) — a
// browser-localStorage-backed one for web, a real std::fs one for any
// native target (desktop GUI or the bsb CLI), both flat-file-shaped via
// vault_format. Re-exported here under one name so ActiveStorage's match
// arms below read the same either way as SupabaseStorage.
#[cfg(feature = "native")]
pub use super::local_desktop::LocalStorage;
#[cfg(not(feature = "native"))]
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
