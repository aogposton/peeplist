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
    // The Local vault has no real backend yet (that's Phase 1d/1e — flat-file
    // format + localStorage/filesystem backends). Until then it's a stub:
    // reads return empty results (so the UI just shows "nothing yet" rather
    // than an error), writes return this so nothing is silently lost.
    NotImplemented,
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageError::Network(e) => write!(f, "{e}"),
            StorageError::NotImplemented => write!(f, "the Local vault isn't implemented yet"),
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

    pub async fn delete_moment(&self, moment: MomentType) -> Result<(), StorageError> {
        Ok(super::moment::deleteMoment(moment, self.token.clone()).await?)
    }

    pub async fn delete_entity(&self, id: String) -> Result<(), StorageError> {
        Ok(super::entity::deleteEntity(id, self.token.clone()).await?)
    }

    pub async fn delete_reaction(&self, reaction: ReactionType) -> Result<(), StorageError> {
        Ok(super::moment::deleteReaction(reaction, self.token.clone()).await?)
    }
}

// Stub — see the StorageError::NotImplemented doc comment above. Gets
// replaced by the real flat-file-backed local vault in Phase 1d/1e without
// changing this surface, so call sites won't need to move again.
pub struct LocalStorage;

impl LocalStorage {
    pub fn new() -> Self {
        Self
    }

    pub async fn get_moments(&self) -> Result<Vec<MomentType>, StorageError> {
        Ok(vec![])
    }

    pub async fn get_entities(&self) -> Result<Vec<EntityType>, StorageError> {
        Ok(vec![])
    }

    pub async fn get_entity_types(&self) -> Result<Vec<EntityTypeType>, StorageError> {
        Ok(vec![])
    }

    pub async fn create_moment(&self, _m: NewMomentType) -> Result<MomentType, StorageError> {
        Err(StorageError::NotImplemented)
    }

    pub async fn create_entity(&self, _e: NewEntityType) -> Result<EntityType, StorageError> {
        Err(StorageError::NotImplemented)
    }

    pub async fn create_reaction(&self, _r: NewReactionType) -> Result<ReactionType, StorageError> {
        Err(StorageError::NotImplemented)
    }

    pub async fn update_moment_field(&self, _id: String, _field: &str, _value: Value) -> Result<(), StorageError> {
        Err(StorageError::NotImplemented)
    }

    pub async fn update_entity_field(&self, _id: String, _field: &str, _value: Value) -> Result<(), StorageError> {
        Err(StorageError::NotImplemented)
    }

    pub async fn delete_moment(&self, _moment: MomentType) -> Result<(), StorageError> {
        Err(StorageError::NotImplemented)
    }

    pub async fn delete_entity(&self, _id: String) -> Result<(), StorageError> {
        Err(StorageError::NotImplemented)
    }

    pub async fn delete_reaction(&self, _reaction: ReactionType) -> Result<(), StorageError> {
        Err(StorageError::NotImplemented)
    }
}

pub enum ActiveStorage {
    Local(LocalStorage),
    Supabase(SupabaseStorage),
}

impl ActiveStorage {
    pub fn for_vault(vault: VaultKind, token: Option<String>) -> Self {
        match vault {
            VaultKind::Synced => match token {
                Some(t) => ActiveStorage::Supabase(SupabaseStorage::new(t)),
                // Can't sync logged out — fall back rather than firing a
                // doomed authenticated request.
                None => ActiveStorage::Local(LocalStorage::new()),
            },
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
}
