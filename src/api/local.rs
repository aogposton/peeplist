// Local-first pivot, Phase 1e web slice (see /Users/aogposton/.claude/plans/joyful-brewing-feather.md
// §1e and memory reference_local_first_pivot_plan). The real Local vault
// backend for the web build: one `localStorage` key per entity, holding the
// exact same rendered YAML-frontmatter-plus-markdown text the eventual
// desktop `std::fs` backend will write to disk (see api/vault_format.rs) —
// "export your data" later is then a literal file copy or browser download
// of these same values, no transformation step to get wrong.
//
// The desktop filesystem backend is explicitly out of scope here (a
// separate follow-up) — this file only ever touches `web_sys::Storage`.

use crate::types::*;
use crate::api::storage::StorageError;
use crate::api::vault_format::{self, ParsedEntityFile, TrashEntry, VaultMeta, LOCAL_SELF_ENTITY_ID, VAULT_SCHEMA_VERSION};
use serde_json::Value;
use uuid::Uuid;
use web_sys::Storage;

const KEY_PREFIX: &str = "peeplist_vault:entity:";
const TRASH_KEY: &str = "peeplist_vault:trash";
const VAULT_META_KEY: &str = "peeplist_vault:meta";

// A first-pass default list — get_entity_types() below adds to this
// whatever type names are actually in use across the vault, per §1d ("the
// distinct set of strings in use across the vault plus a small built-in
// default list"). Not meant to be exhaustive, just non-empty on a brand
// new vault so the "New Entity" type dropdown isn't blank.
const DEFAULT_ENTITY_TYPES: &[&str] = &["Friend", "Family", "Colleague", "Partner"];

fn local_storage() -> Result<Storage, StorageError> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .ok_or_else(|| StorageError::Local("localStorage isn't available".to_string()))
}

fn entity_key(id: &str) -> String {
    format!("{KEY_PREFIX}{id}")
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

// EntityType/MomentType's `created_at` fields are `#[serde(skip_serializing)]`
// — that made sense for the Supabase HTTP boundary (never send a
// client-side timestamp back over a value the server assigns), but it means
// a plain `serde_json::to_value(&record)` silently drops created_at too.
// Since the generic field-patch below works by round-tripping the whole
// record through a Value, that field has to be put back before merging in
// the requested change, or every single local field-patch would quietly
// blank the record's creation time back to "" the moment `default` kicks in
// on deserialize.
fn to_patchable_value<T: serde::Serialize>(record: &T, created_at: &str) -> Result<Value, StorageError> {
    let mut json = serde_json::to_value(record).map_err(|e| StorageError::Local(e.to_string()))?;
    if let Some(obj) = json.as_object_mut() {
        obj.insert("created_at".to_string(), Value::String(created_at.to_string()));
    }
    Ok(json)
}

pub struct LocalStorage;

impl LocalStorage {
    pub fn new() -> Self {
        Self
    }

    fn entity_ids(&self, storage: &Storage) -> Vec<String> {
        let len = storage.length().unwrap_or(0);
        (0..len)
            .filter_map(|i| storage.key(i).ok().flatten())
            .filter_map(|key| key.strip_prefix(KEY_PREFIX).map(str::to_string))
            .collect()
    }

    fn load(&self, storage: &Storage, id: &str) -> Option<ParsedEntityFile> {
        let raw = storage.get_item(&entity_key(id)).ok().flatten()?;
        vault_format::parse_entity_file(&raw).ok()
    }

    fn save(&self, storage: &Storage, entity: &EntityType, moments: &[MomentType], body: &str) -> Result<(), StorageError> {
        let rendered = vault_format::render_entity_file(entity, moments, body);
        storage
            .set_item(&entity_key(&entity.id), &rendered)
            .map_err(|_| StorageError::Local("failed to write to localStorage".to_string()))
    }

    fn all(&self, storage: &Storage) -> Vec<ParsedEntityFile> {
        self.entity_ids(storage).iter().filter_map(|id| self.load(storage, id)).collect()
    }

    // A vault opens idempotently — first-ever access on a fresh browser
    // profile creates the self entity (self.md's in-app equivalent) rather
    // than erroring, so there's zero setup before the app is usable.
    fn ensure_self_entity(&self, storage: &Storage) -> Result<(), StorageError> {
        if storage.get_item(&entity_key(LOCAL_SELF_ENTITY_ID)).ok().flatten().is_some() {
            return Ok(());
        }
        let self_entity = EntityType {
            id: LOCAL_SELF_ENTITY_ID.to_string(),
            name: "Self".to_string(),
            entity_type_id: None,
            created_at: now(),
            drift: 2.0,
            metadata: None,
        };
        self.save(storage, &self_entity, &[], "")
    }

    // Same idempotent-open pattern as ensure_self_entity: writes
    // .peeplist/vault.yaml's in-storage equivalent on first access. Only
    // one schema version exists today, so the compatibility check is a
    // no-op in practice — it's here so a future version bump has
    // somewhere to actually land instead of silently misparsing an old
    // vault.
    fn ensure_vault_meta(&self, storage: &Storage) -> Result<(), StorageError> {
        if let Some(raw) = storage.get_item(VAULT_META_KEY).ok().flatten() {
            if let Ok(meta) = vault_format::parse_vault_meta(&raw) {
                if meta.schema_version > VAULT_SCHEMA_VERSION {
                    log::warn!(
                        "vault schema version {} is newer than this app build supports ({}) — proceeding, but some data may not round-trip correctly",
                        meta.schema_version, VAULT_SCHEMA_VERSION
                    );
                }
                return Ok(());
            }
        }
        let meta = VaultMeta {
            schema_version: VAULT_SCHEMA_VERSION,
            created_at: now(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
        };
        let rendered = vault_format::render_vault_meta(&meta).map_err(|e| StorageError::Local(e.to_string()))?;
        storage
            .set_item(VAULT_META_KEY, &rendered)
            .map_err(|_| StorageError::Local("failed to write vault metadata".to_string()))
    }

    fn load_trash(&self, storage: &Storage) -> Vec<TrashEntry> {
        storage
            .get_item(TRASH_KEY)
            .ok()
            .flatten()
            .and_then(|raw| vault_format::parse_trash(&raw).ok())
            .unwrap_or_default()
    }

    fn append_trash(&self, storage: &Storage, entry: TrashEntry) -> Result<(), StorageError> {
        let mut entries = self.load_trash(storage);
        entries.push(entry);
        let rendered = vault_format::render_trash(&entries).map_err(|e| StorageError::Local(e.to_string()))?;
        storage
            .set_item(TRASH_KEY, &rendered)
            .map_err(|_| StorageError::Local("failed to write to trash".to_string()))
    }

    pub async fn get_moments(&self) -> Result<Vec<MomentType>, StorageError> {
        let storage = local_storage()?;
        self.ensure_self_entity(&storage)?;
        self.ensure_vault_meta(&storage)?;
        Ok(self.all(&storage).into_iter().flat_map(|f| f.moments).collect())
    }

    pub async fn get_entities(&self) -> Result<Vec<EntityType>, StorageError> {
        let storage = local_storage()?;
        self.ensure_self_entity(&storage)?;
        self.ensure_vault_meta(&storage)?;
        Ok(self.all(&storage).into_iter().map(|f| f.entity).collect())
    }

    // Web has no way to get data back out today — the desktop build already
    // writes real files under ~/Documents/Peeplist, but a web vault only
    // exists in localStorage, with no export path at all. Bundles every
    // entity's exact rendered file content (frontmatter + freeform body,
    // byte-for-byte what render_entity_file produces — same thing the
    // desktop build would write to disk for the same data) so nothing is
    // lost, not even notes typed into a file's freeform section.
    pub async fn export_all(&self) -> Result<Vec<(String, String)>, StorageError> {
        let storage = local_storage()?;
        self.ensure_self_entity(&storage)?;
        Ok(self.all(&storage).into_iter().map(|f| {
            let filename = if f.entity.id == LOCAL_SELF_ENTITY_ID {
                vault_format::SELF_FILENAME.to_string()
            } else {
                vault_format::entity_filename(&f.entity.name, &f.entity.id)
            };
            let rendered = vault_format::render_entity_file(&f.entity, &f.moments, &f.body);
            (filename, rendered)
        }).collect())
    }

    pub async fn get_entity_types(&self) -> Result<Vec<EntityTypeType>, StorageError> {
        let storage = local_storage()?;
        let mut names: Vec<String> = self.all(&storage).into_iter().filter_map(|f| f.entity.entity_type_id).collect();
        for default in DEFAULT_ENTITY_TYPES {
            if !names.iter().any(|n| n == default) {
                names.push(default.to_string());
            }
        }
        names.sort();
        names.dedup();
        Ok(names.into_iter().map(|name| EntityTypeType { id: name.clone(), name }).collect())
    }

    pub async fn create_moment(&self, m: NewMomentType) -> Result<MomentType, StorageError> {
        let storage = local_storage()?;
        let mut file = self
            .load(&storage, &m.entity_id)
            .ok_or_else(|| StorageError::Local(format!("no such entity in this vault: {}", m.entity_id)))?;
        let moment = MomentType {
            id: Uuid::new_v4().to_string(),
            title: m.title,
            description: m.description,
            gravity: m.gravity,
            entity_id: m.entity_id,
            moment_type_id: m.moment_type_id,
            due_at: None,
            completed_at: None,
            deleted_at: None,
            reactions: None,
            created_at: now(),
            depends_on: None,
            metadata: None,
        };
        file.moments.push(moment.clone());
        self.save(&storage, &file.entity, &file.moments, &file.body)?;
        Ok(moment)
    }

    pub async fn create_entity(&self, e: NewEntityType) -> Result<EntityType, StorageError> {
        let storage = local_storage()?;
        let entity = EntityType {
            id: Uuid::new_v4().to_string(),
            name: e.name,
            entity_type_id: e.entity_type_id,
            created_at: now(),
            drift: 2.0,
            metadata: e.metadata,
        };
        self.save(&storage, &entity, &[], "")?;
        Ok(entity)
    }

    pub async fn create_reaction(&self, r: NewReactionType) -> Result<ReactionType, StorageError> {
        let storage = local_storage()?;
        for id in self.entity_ids(&storage) {
            let Some(mut file) = self.load(&storage, &id) else { continue };
            let Some(m) = file.moments.iter_mut().find(|m| m.id == r.moment_id) else { continue };
            let reaction = ReactionType {
                id: Uuid::new_v4().to_string(),
                description: r.description,
                moment_id: r.moment_id,
                value: r.value,
            };
            m.reactions.get_or_insert_with(Vec::new).push(reaction.clone());
            self.save(&storage, &file.entity, &file.moments, &file.body)?;
            return Ok(reaction);
        }
        Err(StorageError::Local(format!("no such moment in this vault: {}", r.moment_id)))
    }

    pub async fn update_moment_field(&self, id: String, field: &str, value: Value) -> Result<(), StorageError> {
        let storage = local_storage()?;
        for entity_id in self.entity_ids(&storage) {
            let Some(mut file) = self.load(&storage, &entity_id) else { continue };
            let Some(pos) = file.moments.iter().position(|m| m.id == id) else { continue };
            let mut json = to_patchable_value(&file.moments[pos], &file.moments[pos].created_at)?;
            if let Some(obj) = json.as_object_mut() {
                obj.insert(field.to_string(), value);
            }
            file.moments[pos] = serde_json::from_value(json).map_err(|e| StorageError::Local(e.to_string()))?;
            return self.save(&storage, &file.entity, &file.moments, &file.body);
        }
        Err(StorageError::Local(format!("no such moment in this vault: {id}")))
    }

    pub async fn update_entity_field(&self, id: String, field: &str, value: Value) -> Result<(), StorageError> {
        let storage = local_storage()?;
        let mut file = self.load(&storage, &id).ok_or_else(|| StorageError::Local(format!("no such entity in this vault: {id}")))?;
        let mut json = to_patchable_value(&file.entity, &file.entity.created_at)?;
        if let Some(obj) = json.as_object_mut() {
            obj.insert(field.to_string(), value);
        }
        file.entity = serde_json::from_value(json).map_err(|e| StorageError::Local(e.to_string()))?;
        self.save(&storage, &file.entity, &file.moments, &file.body)
    }

    // Soft delete: the record is appended to the trash log (see
    // vault_format::TrashEntry) before being removed from the visible
    // file, consistent with the product's "never actually throw your
    // data away" stance — no recovery UI reads this back yet, but the
    // record isn't gone.
    pub async fn delete_moment(&self, moment: MomentType) -> Result<(), StorageError> {
        let storage = local_storage()?;
        let mut file = self
            .load(&storage, &moment.entity_id)
            .ok_or_else(|| StorageError::Local(format!("no such entity in this vault: {}", moment.entity_id)))?;
        if let Some(m) = file.moments.iter().find(|m| m.id == moment.id) {
            let entry = vault_format::moment_to_entry(m);
            // Best-effort: trash-logging is bookkeeping, not the thing the
            // user actually asked for. It used to be able to silently veto
            // the whole delete via `?` if it failed for any reason — the
            // record still not being fully recoverable-from-trash is a much
            // smaller problem than "delete does nothing."
            if let Err(e) = self.append_trash(&storage, TrashEntry::Moment {
                entity_id: moment.entity_id.clone(),
                moment: entry,
                deleted_at: now(),
            }) {
                log::warn!("Failed to log deleted moment to trash (deleting anyway): {e}");
            }
        }
        file.moments.retain(|m| m.id != moment.id);
        self.save(&storage, &file.entity, &file.moments, &file.body)
    }

    pub async fn delete_entity(&self, id: String) -> Result<(), StorageError> {
        let storage = local_storage()?;
        if let Some(file) = self.load(&storage, &id) {
            let doc = vault_format::entity_to_doc(&file.entity, &file.moments);
            if let Err(e) = self.append_trash(&storage, TrashEntry::Entity { entity: doc, deleted_at: now() }) {
                log::warn!("Failed to log deleted entity to trash (deleting anyway): {e}");
            }
        }
        storage
            .remove_item(&entity_key(&id))
            .map_err(|_| StorageError::Local("failed to remove from localStorage".to_string()))
    }

    pub async fn delete_reaction(&self, reaction: ReactionType) -> Result<(), StorageError> {
        let storage = local_storage()?;
        for entity_id in self.entity_ids(&storage) {
            let Some(mut file) = self.load(&storage, &entity_id) else { continue };
            let Some(m) = file.moments.iter_mut().find(|m| m.id == reaction.moment_id) else { continue };
            let Some(reactions) = m.reactions.as_mut() else { continue };
            let before = reactions.len();
            reactions.retain(|r| r.id != reaction.id);
            if reactions.len() == before {
                continue;
            }
            return self.save(&storage, &file.entity, &file.moments, &file.body);
        }
        Err(StorageError::Local(format!("no such reaction in this vault: {}", reaction.id)))
    }
}
