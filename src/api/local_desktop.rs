// Local-first pivot, Phase 1e desktop slice (see /Users/aogposton/.claude/plans/joyful-brewing-feather.md
// §1e and memory reference_local_first_pivot_plan). The real Local vault
// backend for the desktop build: plain `std::fs`, one file per entity under
// `~/Documents/Peeplist/people/`, holding exactly what `vault_format`
// renders — the same text the web build's `localStorage`-backed
// LocalStorage (src/api/local.rs) stores under one key per entity, so
// "export your data" is a literal file copy either way.
//
// Deliberately minimal otherwise: no vault-path configuration UI
// (hardcoded default root only). Lookups scan `people/*.md` and parse
// each file's frontmatter rather than trusting the filename, since §1d's
// own filename convention is explicit that the slug is a creation-time
// hint, not the source of truth — the id inside the file is. Deletes are
// soft (trash.yaml at the vault root, see local.rs's matching comment).

use crate::types::*;
use crate::api::storage::StorageError;
use crate::api::vault_format::{self, ParsedEntityFile, TrashEntry, VaultMeta, LOCAL_SELF_ENTITY_ID, SELF_FILENAME, VAULT_SCHEMA_VERSION};
use serde_json::Value;
use uuid::Uuid;
use std::fs;
use std::path::{Path, PathBuf};

fn vault_root() -> Result<PathBuf, StorageError> {
    let dirs = directories::UserDirs::new()
        .ok_or_else(|| StorageError::Local("couldn't resolve your home directory".to_string()))?;
    let docs = dirs
        .document_dir()
        .ok_or_else(|| StorageError::Local("couldn't resolve your Documents folder".to_string()))?;
    Ok(docs.join("Peeplist"))
}

fn people_dir() -> Result<PathBuf, StorageError> {
    let dir = vault_root()?.join("people");
    fs::create_dir_all(&dir).map_err(|e| StorageError::Local(e.to_string()))?;
    Ok(dir)
}

fn self_path() -> Result<PathBuf, StorageError> {
    Ok(vault_root()?.join(SELF_FILENAME))
}

fn trash_path() -> Result<PathBuf, StorageError> {
    Ok(vault_root()?.join("trash.yaml"))
}

fn vault_meta_path() -> Result<PathBuf, StorageError> {
    Ok(vault_root()?.join("vault.yaml"))
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

// See local.rs's identical helper for why created_at has to be put back
// before merging a field-patch through a serde_json::Value round-trip.
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

    fn person_paths(&self) -> Result<Vec<PathBuf>, StorageError> {
        let dir = people_dir()?;
        let entries = fs::read_dir(&dir).map_err(|e| StorageError::Local(e.to_string()))?;
        Ok(entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|ext| ext == "md").unwrap_or(false))
            .collect())
    }

    fn all_paths(&self) -> Result<Vec<PathBuf>, StorageError> {
        let mut paths = self.person_paths()?;
        let self_p = self_path()?;
        if self_p.exists() {
            paths.push(self_p);
        }
        Ok(paths)
    }

    fn load_path(&self, path: &Path) -> Option<ParsedEntityFile> {
        let raw = fs::read_to_string(path).ok()?;
        vault_format::parse_entity_file(&raw).ok()
    }

    fn all(&self) -> Result<Vec<ParsedEntityFile>, StorageError> {
        Ok(self.all_paths()?.iter().filter_map(|p| self.load_path(p)).collect())
    }

    // §1d: renaming a person doesn't rename the file — the id suffix is
    // canonical, so lookups go by parsing each file's own `id:` field
    // rather than trusting the filename.
    fn find(&self, id: &str) -> Result<Option<(PathBuf, ParsedEntityFile)>, StorageError> {
        if id == LOCAL_SELF_ENTITY_ID {
            let p = self_path()?;
            return Ok(self.load_path(&p).map(|f| (p, f)));
        }
        for path in self.person_paths()? {
            if let Some(file) = self.load_path(&path) {
                if file.entity.id == id {
                    return Ok(Some((path, file)));
                }
            }
        }
        Ok(None)
    }

    fn path_for_new(&self, entity: &EntityType) -> Result<PathBuf, StorageError> {
        if entity.id == LOCAL_SELF_ENTITY_ID {
            return self_path();
        }
        Ok(people_dir()?.join(vault_format::entity_filename(&entity.name, &entity.id)))
    }

    // Atomic write per §1d ("<file>.tmp + rename prevent corruption on
    // crash mid-write").
    fn save(&self, path: &Path, entity: &EntityType, moments: &[MomentType], body: &str) -> Result<(), StorageError> {
        let rendered = vault_format::render_entity_file(entity, moments, body);
        let tmp = path.with_extension("md.tmp");
        fs::write(&tmp, rendered).map_err(|e| StorageError::Local(e.to_string()))?;
        fs::rename(&tmp, path).map_err(|e| StorageError::Local(e.to_string()))
    }

    fn ensure_self_entity(&self) -> Result<(), StorageError> {
        let p = self_path()?;
        if p.exists() {
            return Ok(());
        }
        let self_entity = EntityType {
            id: LOCAL_SELF_ENTITY_ID.to_string(),
            name: "Self".to_string(),
            entity_type_id: None,
            parent_entity_id: None,
            created_at: now(),
            drift: 2.0,
            metadata: None,
        };
        self.save(&p, &self_entity, &[], "")
    }

    // See local.rs's identical helper — only one schema version exists
    // today, so the compatibility check is a no-op in practice, but it's
    // here so a future version bump has somewhere to land.
    fn ensure_vault_meta(&self) -> Result<(), StorageError> {
        let p = vault_meta_path()?;
        if let Ok(raw) = fs::read_to_string(&p) {
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
        fs::write(&p, rendered).map_err(|e| StorageError::Local(e.to_string()))
    }

    fn load_trash(&self) -> Vec<TrashEntry> {
        trash_path()
            .ok()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|raw| vault_format::parse_trash(&raw).ok())
            .unwrap_or_default()
    }

    fn append_trash(&self, entry: TrashEntry) -> Result<(), StorageError> {
        let mut entries = self.load_trash();
        entries.push(entry);
        let rendered = vault_format::render_trash(&entries).map_err(|e| StorageError::Local(e.to_string()))?;
        let p = trash_path()?;
        let tmp = p.with_extension("yaml.tmp");
        fs::write(&tmp, rendered).map_err(|e| StorageError::Local(e.to_string()))?;
        fs::rename(&tmp, &p).map_err(|e| StorageError::Local(e.to_string()))
    }

    pub async fn get_moments(&self) -> Result<Vec<MomentType>, StorageError> {
        self.ensure_self_entity()?;
        self.ensure_vault_meta()?;
        Ok(self.all()?.into_iter().flat_map(|f| f.moments).collect())
    }

    pub async fn get_entities(&self) -> Result<Vec<EntityType>, StorageError> {
        self.ensure_self_entity()?;
        self.ensure_vault_meta()?;
        Ok(self.all()?.into_iter().map(|f| f.entity).collect())
    }

    pub async fn get_entity_types(&self) -> Result<Vec<EntityTypeType>, StorageError> {
        const DEFAULT_ENTITY_TYPES: &[&str] = &["Friend", "Family", "Colleague", "Partner"];
        let mut names: Vec<String> = self.all()?.into_iter().filter_map(|f| f.entity.entity_type_id).collect();
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
        let (path, mut file) = self
            .find(&m.entity_id)?
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
        self.save(&path, &file.entity, &file.moments, &file.body)?;
        Ok(moment)
    }

    pub async fn create_entity(&self, e: NewEntityType) -> Result<EntityType, StorageError> {
        let entity = EntityType {
            id: Uuid::new_v4().to_string(),
            name: e.name,
            entity_type_id: e.entity_type_id,
            parent_entity_id: e.parent_entity_id,
            created_at: now(),
            drift: 2.0,
            metadata: e.metadata,
        };
        let path = self.path_for_new(&entity)?;
        self.save(&path, &entity, &[], "")?;
        Ok(entity)
    }

    pub async fn create_reaction(&self, r: NewReactionType) -> Result<ReactionType, StorageError> {
        for path in self.person_paths()?.into_iter().chain(self_path().into_iter().filter(|p| p.exists())) {
            let Some(mut file) = self.load_path(&path) else { continue };
            let Some(m) = file.moments.iter_mut().find(|m| m.id == r.moment_id) else { continue };
            let reaction = ReactionType {
                id: Uuid::new_v4().to_string(),
                description: r.description,
                moment_id: r.moment_id,
                value: r.value,
            };
            m.reactions.get_or_insert_with(Vec::new).push(reaction.clone());
            self.save(&path, &file.entity, &file.moments, &file.body)?;
            return Ok(reaction);
        }
        Err(StorageError::Local(format!("no such moment in this vault: {}", r.moment_id)))
    }

    pub async fn update_moment_field(&self, id: String, field: &str, value: Value) -> Result<(), StorageError> {
        for path in self.person_paths()?.into_iter().chain(self_path().into_iter().filter(|p| p.exists())) {
            let Some(mut file) = self.load_path(&path) else { continue };
            let Some(pos) = file.moments.iter().position(|m| m.id == id) else { continue };
            let mut json = to_patchable_value(&file.moments[pos], &file.moments[pos].created_at)?;
            if let Some(obj) = json.as_object_mut() {
                obj.insert(field.to_string(), value);
            }
            file.moments[pos] = serde_json::from_value(json).map_err(|e| StorageError::Local(e.to_string()))?;
            return self.save(&path, &file.entity, &file.moments, &file.body);
        }
        Err(StorageError::Local(format!("no such moment in this vault: {id}")))
    }

    // See local.rs's identical method for why entity_id can't go through
    // update_moment_field above — a moment's home file *is* the entity it
    // belongs to here, so this actually moves it between files rather than
    // just patching the field.
    pub async fn reassign_moment_entity(&self, moment_id: String, new_entity_id: String) -> Result<(), StorageError> {
        let (new_path, mut new_file) = self.find(&new_entity_id)?
            .ok_or_else(|| StorageError::Local(format!("no such entity in this vault: {new_entity_id}")))?;

        if new_file.moments.iter().any(|m| m.id == moment_id) {
            return Ok(());
        }

        for path in self.person_paths()?.into_iter().chain(self_path().into_iter().filter(|p| p.exists())) {
            let Some(mut old_file) = self.load_path(&path) else { continue };
            let Some(pos) = old_file.moments.iter().position(|m| m.id == moment_id) else { continue };
            let mut moment = old_file.moments.remove(pos);
            moment.entity_id = new_entity_id.clone();
            self.save(&path, &old_file.entity, &old_file.moments, &old_file.body)?;
            new_file.moments.push(moment);
            return self.save(&new_path, &new_file.entity, &new_file.moments, &new_file.body);
        }
        Err(StorageError::Local(format!("no such moment in this vault: {moment_id}")))
    }

    pub async fn update_entity_field(&self, id: String, field: &str, value: Value) -> Result<(), StorageError> {
        let (path, mut file) = self.find(&id)?.ok_or_else(|| StorageError::Local(format!("no such entity in this vault: {id}")))?;
        let mut json = to_patchable_value(&file.entity, &file.entity.created_at)?;
        if let Some(obj) = json.as_object_mut() {
            obj.insert(field.to_string(), value);
        }
        file.entity = serde_json::from_value(json).map_err(|e| StorageError::Local(e.to_string()))?;
        self.save(&path, &file.entity, &file.moments, &file.body)
    }

    // Soft delete — see local.rs's matching comment. Record goes to
    // trash.yaml before the file is touched.
    pub async fn delete_moment(&self, moment: MomentType) -> Result<(), StorageError> {
        let (path, mut file) = self
            .find(&moment.entity_id)?
            .ok_or_else(|| StorageError::Local(format!("no such entity in this vault: {}", moment.entity_id)))?;
        if let Some(m) = file.moments.iter().find(|m| m.id == moment.id) {
            let entry = vault_format::moment_to_entry(m);
            // Best-effort — see local.rs's matching comment: trash-logging
            // used to be able to silently veto the whole delete via `?` if
            // it failed for any reason.
            if let Err(e) = self.append_trash(TrashEntry::Moment {
                entity_id: moment.entity_id.clone(),
                moment: entry,
                deleted_at: now(),
            }) {
                log::warn!("Failed to log deleted moment to trash (deleting anyway): {e}");
            }
        }
        file.moments.retain(|m| m.id != moment.id);
        self.save(&path, &file.entity, &file.moments, &file.body)
    }

    pub async fn delete_entity(&self, id: String) -> Result<(), StorageError> {
        let (path, file) = self.find(&id)?.ok_or_else(|| StorageError::Local(format!("no such entity in this vault: {id}")))?;
        let doc = vault_format::entity_to_doc(&file.entity, &file.moments);
        if let Err(e) = self.append_trash(TrashEntry::Entity { entity: doc, deleted_at: now() }) {
            log::warn!("Failed to log deleted entity to trash (deleting anyway): {e}");
        }
        fs::remove_file(&path).map_err(|e| StorageError::Local(e.to_string()))
    }

    // See local.rs's matching methods for the full rationale — same
    // TrashEntry::Moment-only scope (whole-entity restore isn't built yet).
    pub async fn get_deleted_moments(&self) -> Result<Vec<MomentType>, StorageError> {
        let mut entries: Vec<(MomentType, String)> = self.load_trash().into_iter()
            .filter_map(|entry| match entry {
                TrashEntry::Moment { entity_id, moment, deleted_at } => {
                    Some((vault_format::entry_to_moment(&moment, &entity_id), deleted_at))
                }
                TrashEntry::Entity { .. } => None,
            })
            .collect();
        entries.sort_by(|a, b| b.1.cmp(&a.1));
        Ok(entries.into_iter().map(|(m, _)| m).collect())
    }

    pub async fn restore_moment(&self, moment_id: String) -> Result<(), StorageError> {
        let mut trash = self.load_trash();
        let pos = trash.iter().position(|entry| matches!(
            entry,
            TrashEntry::Moment { moment, .. } if moment.id == moment_id
        )).ok_or_else(|| StorageError::Local("that moment isn't in the trash".to_string()))?;

        let TrashEntry::Moment { entity_id, moment, .. } = trash.remove(pos) else {
            unreachable!("position was matched on TrashEntry::Moment above");
        };

        let (path, mut file) = self.find(&entity_id)?
            .ok_or_else(|| StorageError::Local("the entity this moment belonged to no longer exists in this vault".to_string()))?;
        file.moments.push(vault_format::entry_to_moment(&moment, &entity_id));
        self.save(&path, &file.entity, &file.moments, &file.body)?;

        let rendered = vault_format::render_trash(&trash).map_err(|e| StorageError::Local(e.to_string()))?;
        let trash_p = trash_path()?;
        let tmp = trash_p.with_extension("yaml.tmp");
        fs::write(&tmp, rendered).map_err(|e| StorageError::Local(e.to_string()))?;
        fs::rename(&tmp, &trash_p).map_err(|e| StorageError::Local(e.to_string()))
    }

    pub async fn delete_reaction(&self, reaction: ReactionType) -> Result<(), StorageError> {
        for path in self.person_paths()?.into_iter().chain(self_path().into_iter().filter(|p| p.exists())) {
            let Some(mut file) = self.load_path(&path) else { continue };
            let Some(m) = file.moments.iter_mut().find(|m| m.id == reaction.moment_id) else { continue };
            let Some(reactions) = m.reactions.as_mut() else { continue };
            let before = reactions.len();
            reactions.retain(|r| r.id != reaction.id);
            if reactions.len() == before {
                continue;
            }
            return self.save(&path, &file.entity, &file.moments, &file.body);
        }
        Err(StorageError::Local(format!("no such reaction in this vault: {}", reaction.id)))
    }
}
