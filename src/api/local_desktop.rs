// Local-first pivot, Phase 1e desktop slice (see /Users/aogposton/.claude/plans/joyful-brewing-feather.md
// §1e and memory reference_local_first_pivot_plan). The real Local vault
// backend for the desktop build: plain `std::fs`, one file per entity under
// `~/Documents/Peeplist/people/`, holding exactly what `vault_format`
// renders — the same text the web build's `localStorage`-backed
// LocalStorage (src/api/local.rs) stores under one key per entity, so
// "export your data" is a literal file copy either way.
//
// Deliberately minimal, matching the same scope as the web slice: no
// `.peeplist/trash.yaml` wiring (deletes are hard deletes, see local.rs's
// matching comment), no vault-path configuration UI (hardcoded default
// root only). Lookups scan `people/*.md` and parse each file's frontmatter
// rather than trusting the filename, since §1d's own filename convention
// is explicit that the slug is a creation-time hint, not the source of
// truth — the id inside the file is.

use crate::types::*;
use crate::api::storage::StorageError;
use crate::api::vault_format::{self, ParsedEntityFile, LOCAL_SELF_ENTITY_ID, SELF_FILENAME};
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
            created_at: now(),
            drift: 2.0,
            metadata: None,
        };
        self.save(&p, &self_entity, &[], "")
    }

    pub async fn get_moments(&self) -> Result<Vec<MomentType>, StorageError> {
        self.ensure_self_entity()?;
        Ok(self.all()?.into_iter().flat_map(|f| f.moments).collect())
    }

    pub async fn get_entities(&self) -> Result<Vec<EntityType>, StorageError> {
        self.ensure_self_entity()?;
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

    pub async fn update_entity_field(&self, id: String, field: &str, value: Value) -> Result<(), StorageError> {
        let (path, mut file) = self.find(&id)?.ok_or_else(|| StorageError::Local(format!("no such entity in this vault: {id}")))?;
        let mut json = to_patchable_value(&file.entity, &file.entity.created_at)?;
        if let Some(obj) = json.as_object_mut() {
            obj.insert(field.to_string(), value);
        }
        file.entity = serde_json::from_value(json).map_err(|e| StorageError::Local(e.to_string()))?;
        self.save(&path, &file.entity, &file.moments, &file.body)
    }

    // Hard delete for now — see local.rs's matching comment.
    pub async fn delete_moment(&self, moment: MomentType) -> Result<(), StorageError> {
        let (path, mut file) = self
            .find(&moment.entity_id)?
            .ok_or_else(|| StorageError::Local(format!("no such entity in this vault: {}", moment.entity_id)))?;
        file.moments.retain(|m| m.id != moment.id);
        self.save(&path, &file.entity, &file.moments, &file.body)
    }

    pub async fn delete_entity(&self, id: String) -> Result<(), StorageError> {
        let (path, _) = self.find(&id)?.ok_or_else(|| StorageError::Local(format!("no such entity in this vault: {id}")))?;
        fs::remove_file(&path).map_err(|e| StorageError::Local(e.to_string()))
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
