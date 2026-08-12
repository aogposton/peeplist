// Local-first pivot, Phase 1d (see /Users/aogposton/.claude/plans/joyful-brewing-feather.md
// §1d and memory reference_local_first_pivot_plan). This module is the vault
// file *format* only: pure Rust, zero `std::fs`/`web_sys` dependency, so it
// compiles and is testable on every target. It defines how one person's
// entity + moments round-trips to the YAML-frontmatter-plus-markdown text
// described in the plan, and how vault filenames are derived. Nothing here
// reads or writes an actual file or localStorage key — that's Phase 1e's
// job (a desktop `std::fs` backend and a web `localStorage` backend), both
// meant to share this exact module rather than reimplement the format.
//
// Unit of file = one entity, not one moment — see §1d for the reasoning
// (git-friendly diffs scoped to one relationship, the right context-load
// unit for a person's own LLM/agent to reason about one relationship at a
// time). YAML frontmatter holds all structured data; the markdown body
// below it is reserved for free human prose and is never machine-written —
// `parse_entity_file` hands the body back untouched so a future write-back
// can preserve it exactly.

use crate::types::{EntityMetadata, EntityType, MomentMetadata, MomentType, ReactionType, SELF_ENTITY_TYPE_ID};
use serde::{Deserialize, Serialize};

fn default_drift() -> f64 {
    2.0
}

// Reserved id for the always-present self.md file in a local vault. Deliberately
// distinct from types.rs's SELF_ENTITY_ID ("0"), which is the *Supabase*
// self-entity row's id — the two conventions aren't reconciled yet (see
// memory project_self_entity_convention and reference_local_first_pivot_plan).
// That reconciliation is Phase 1e's problem, once a local backend actually
// resolves "which self id am I" at runtime; this module just needs a stable
// constant for building/recognizing the self.md file itself.
pub const LOCAL_SELF_ENTITY_ID: &str = "self";
pub const SELF_FILENAME: &str = "self.md";

pub const VAULT_SCHEMA_VERSION: u32 = 1;

// Separate from VAULT_SCHEMA_VERSION above — a different format (the
// multi-entity backup bundle, see below) with its own version cadence, not
// the per-entity vault-file format's.
pub const BACKUP_SCHEMA_VERSION: u32 = 1;

// Same check as api::storage::is_self_entity, duplicated rather than
// imported — storage.rs already imports LOCAL_SELF_ENTITY_ID *from* this
// module, so importing is_self_entity back the other way would make the two
// modules depend on each other for what's really just a two-line check.
fn is_self_for_backup(entity: &EntityType) -> bool {
    entity.id == LOCAL_SELF_ENTITY_ID || entity.entity_type_id.as_deref() == Some(SELF_ENTITY_TYPE_ID)
}

const BODY_PLACEHOLDER: &str =
    "<!-- Freeform notes below this line are yours — peeplist never rewrites this section. -->\n";

#[derive(Debug)]
pub enum VaultFormatError {
    MissingFrontmatter,
    Yaml(serde_norway::Error),
}

impl std::fmt::Display for VaultFormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VaultFormatError::MissingFrontmatter => {
                write!(f, "file doesn't start with a `---` YAML frontmatter block")
            }
            VaultFormatError::Yaml(e) => write!(f, "{e}"),
        }
    }
}

impl From<serde_norway::Error> for VaultFormatError {
    fn from(e: serde_norway::Error) -> Self {
        VaultFormatError::Yaml(e)
    }
}

// --- YAML shape --------------------------------------------------------
//
// One EntityDoc per file. EntityMetadata's fields are flattened to the top
// level (matching the plan's example) rather than nested under a
// `metadata:` key, and omitted entirely when empty. `entity_type` is a
// resolved name string, not a numeric FK — local mode has no entity_types
// lookup table, so EntityType.entity_type_id holds that same string
// directly for vault-sourced entities (see doc_to_entity below).

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EntityDoc {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub entity_type: Option<String>,
    // See EntityType::parent_entity_id (types.rs) — the group entity this
    // one was split out of via individuation, if any.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub parent_entity_id: Option<String>,
    #[serde(default = "default_drift")]
    pub drift: f64,
    #[serde(default)]
    pub created_at: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub moments: Vec<MomentEntry>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MomentEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub gravity: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub due_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scheduled_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub until_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub completed_at: Option<String>,
    // Unlimited, like tags (2026-07-29) — was a single Option<String> before.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub depends_on: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub sort_index: Option<f64>,
    #[serde(default)]
    pub created_at: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub reactions: Vec<ReactionEntry>,
    // Multi-entity moments (2026-07-29) — every additional entity this
    // moment also fully belongs to, besides whichever entity's file it
    // physically lives under. See MomentType::entity_ids.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub additional_entity_ids: Vec<String>,
    // Momentos (2026-08-02, moment_type_id 4 only) — see MomentMetadata's
    // own doc comment in types.rs for what these mean; same fields, just
    // riding this file format's flattened shape instead of a nested blob.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub recurrence_rule: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub momento_completed_occurrences: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub momento_excluded_occurrences: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reveal_lead: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ReactionEntry {
    pub id: String,
    pub description: String,
    pub value: i32,
}

// --- moment_type_id <-> "task"/"promise"/"note"/"momento"/"info" --------
// Mirrors the mapping already implemented as `kind_label` in
// src/components/entity.rs — kept in sync by hand since that one renders
// for display ("Task"/"Promise"/"Note"/"Momento"/"Info") and this one is a
// wire format key ("task"/"promise"/"note"/"momento"/"info"), not worth
// sharing a single function over. moment_type_id 4 ("momento") added
// 2026-08-02 — see scripts/2026-08-02-momento-type.sql for why it's pinned
// to exactly 4. moment_type_id 5 ("info") added 2026-08-03 — a Note
// subtype (see MomentType::moment_type_id's doc comment), no schema change
// needed since it's the same bare integer column momento reused.

fn moment_type_str(moment_type_id: i64) -> &'static str {
    match moment_type_id {
        2 => "promise",
        3 => "note",
        4 => "momento",
        5 => "info",
        _ => "task",
    }
}

pub(crate) fn moment_type_id(kind: &str) -> i64 {
    match kind {
        "promise" => 2,
        "note" => 3,
        "momento" => 4,
        "info" => 5,
        _ => 1,
    }
}

// --- conversions ---------------------------------------------------------

fn reaction_to_entry(r: &ReactionType) -> ReactionEntry {
    ReactionEntry {
        id: r.id.clone(),
        description: r.description.clone(),
        value: r.value,
    }
}

fn entry_to_reaction(entry: &ReactionEntry, moment_id: &str) -> ReactionType {
    ReactionType {
        id: entry.id.clone(),
        description: entry.description.clone(),
        moment_id: moment_id.to_string(),
        value: entry.value,
    }
}

pub(crate) fn moment_to_entry(m: &MomentType) -> MomentEntry {
    let meta = m.metadata.clone().unwrap_or_default();
    MomentEntry {
        id: m.id.clone(),
        kind: moment_type_str(m.moment_type_id).to_string(),
        title: m.title.clone(),
        description: m.description.clone().filter(|d| !d.is_empty()),
        gravity: m.gravity,
        due_at: m.due_at.clone(),
        scheduled_at: meta.scheduled_at,
        until_at: meta.until_at,
        priority: meta.priority,
        project: meta.project,
        completed_at: m.completed_at.clone(),
        depends_on: m.dependency_ids(),
        tags: meta.tags,
        sort_index: meta.sort_index,
        created_at: m.created_at.clone(),
        reactions: m.reactions.as_deref().unwrap_or(&[]).iter().map(reaction_to_entry).collect(),
        additional_entity_ids: meta.additional_entity_ids,
        recurrence_rule: meta.recurrence_rule,
        momento_completed_occurrences: meta.momento_completed_occurrences,
        momento_excluded_occurrences: meta.momento_excluded_occurrences,
        reveal_lead: meta.reveal_lead,
    }
}

// Soft-deleted moments never appear in the visible file (see §1d — they're
// filtered out and appended to trash.yaml instead), so `deleted_at` is
// always None for anything round-tripped through this format.
pub(crate) fn entry_to_moment(entry: &MomentEntry, entity_id: &str) -> MomentType {
    let meta = MomentMetadata {
        tags: entry.tags.clone(),
        sort_index: entry.sort_index,
        priority: entry.priority.clone(),
        project: entry.project.clone(),
        scheduled_at: entry.scheduled_at.clone(),
        until_at: entry.until_at.clone(),
        depends_on: entry.depends_on.clone(),
        additional_entity_ids: entry.additional_entity_ids.clone(),
        recurrence_rule: entry.recurrence_rule.clone(),
        momento_completed_occurrences: entry.momento_completed_occurrences.clone(),
        momento_excluded_occurrences: entry.momento_excluded_occurrences.clone(),
        reveal_lead: entry.reveal_lead.clone(),
    };
    let metadata = if meta == MomentMetadata::default() { None } else { Some(meta) };
    let reactions = entry.reactions.iter().map(|r| entry_to_reaction(r, &entry.id)).collect::<Vec<_>>();
    MomentType {
        id: entry.id.clone(),
        title: entry.title.clone(),
        description: entry.description.clone(),
        gravity: entry.gravity,
        entity_id: entity_id.to_string(),
        moment_type_id: moment_type_id(&entry.kind),
        due_at: entry.due_at.clone(),
        completed_at: entry.completed_at.clone(),
        deleted_at: None,
        reactions: if reactions.is_empty() { None } else { Some(reactions) },
        created_at: entry.created_at.clone(),
        // The vault file format has no updated_at of its own (Local vault
        // has no LWW concept — single-device by definition); created_at is
        // the best available stand-in.
        updated_at: entry.created_at.clone(),
        depends_on: None,
        metadata,
    }
}

pub(crate) fn entity_to_doc(entity: &EntityType, moments: &[MomentType]) -> EntityDoc {
    EntityDoc {
        id: entity.id.clone(),
        name: entity.name.clone(),
        entity_type: entity.entity_type_id.clone(),
        parent_entity_id: entity.parent_entity_id.clone(),
        drift: entity.drift,
        created_at: entity.created_at.clone(),
        moments: moments.iter().map(moment_to_entry).collect(),
    }
}

fn doc_to_entity(doc: &EntityDoc) -> (EntityType, Vec<MomentType>) {
    let entity = EntityType {
        id: doc.id.clone(),
        name: doc.name.clone(),
        entity_type_id: doc.entity_type.clone(),
        parent_entity_id: doc.parent_entity_id.clone(),
        created_at: doc.created_at.clone(),
        updated_at: doc.created_at.clone(),
        drift: doc.drift,
        metadata: Some(EntityMetadata::default()),
    };
    let moments = doc.moments.iter().map(|e| entry_to_moment(e, &doc.id)).collect();
    (entity, moments)
}

// --- filenames -------------------------------------------------------------
//
// Renaming a person does NOT rename the file — the id suffix is canonical
// (stable across renames, git-history-friendly), the slug is only a
// creation-time hint for humans browsing the vault directory.

pub fn entity_filename(name: &str, id: &str) -> String {
    let slug = slug::slugify(name);
    let short_id: String = id.chars().take(8).collect();
    if slug.is_empty() {
        format!("{short_id}.md")
    } else {
        format!("{slug}--{short_id}.md")
    }
}

// --- render / parse ----------------------------------------------------

pub struct ParsedEntityFile {
    pub entity: EntityType,
    pub moments: Vec<MomentType>,
    // Everything after the closing `---`, verbatim — hand back to a future
    // write-back call so a user's freeform notes never get clobbered.
    pub body: String,
}

pub fn render_entity_file(entity: &EntityType, moments: &[MomentType], body: &str) -> String {
    let doc = entity_to_doc(entity, moments);
    let yaml = serde_norway::to_string(&doc)
        .expect("EntityDoc is a plain data struct with no maps/floats that can fail to serialize");
    let body = if body.is_empty() { BODY_PLACEHOLDER } else { body };
    format!("---\n{yaml}---\n\n{body}")
}

pub fn parse_entity_file(content: &str) -> Result<ParsedEntityFile, VaultFormatError> {
    let rest = content.strip_prefix("---\n").ok_or(VaultFormatError::MissingFrontmatter)?;
    // Look for a *bare* `---` line specifically (not just any line that
    // happens to start with it, e.g. a markdown horizontal rule sitting
    // inside a description), trying the most-specific pattern first so a
    // stray `---` inside a field value can't be mistaken for the real
    // fence as long as the real one is present somewhere in the file:
    //   1. our own render() always leaves exactly one blank line between
    //      the fence and the body — match that first, so it's the one
    //      found even if a field value elsewhere contains a lone `---`;
    //   2. a hand-edited file with no blank line before the body;
    //   3. frontmatter is the entire file, no body at all.
    let split = rest
        .find("\n---\n\n")
        .map(|pos| (pos, pos + "\n---\n\n".len()))
        .or_else(|| rest.find("\n---\n").map(|pos| (pos, pos + "\n---\n".len())))
        .or_else(|| rest.strip_suffix("\n---").map(|_| (rest.len() - "\n---".len(), rest.len())));
    let (yaml_end, body_start) = split.ok_or(VaultFormatError::MissingFrontmatter)?;
    let yaml = &rest[..yaml_end];
    let body = &rest[body_start..];
    let doc: EntityDoc = serde_norway::from_str(yaml)?;
    let (entity, moments) = doc_to_entity(&doc);
    Ok(ParsedEntityFile { entity, moments, body: body.to_string() })
}

// --- full-vault backup bundle (2026-08-01) --------------------------------
//
// One downloadable YAML file holding every entity and its moments/reactions
// — a disaster-recovery export/restore path (see memory: a real data-loss
// incident is what prompted this). Unlike EntityDoc/MomentEntry above (which
// this deliberately does NOT reuse, despite an earlier version of this
// having done exactly that) this format never contains a real database id —
// every cross-reference is a small file-relative reference number, and
// entity_type is always a resolved literal name, never a raw entity_types
// foreign-key string. A shared backend's auto-incrementing bigint ids
// leaking through a file meant to be downloaded/shared/re-uploaded is a real
// information disclosure (it reveals things like total row counts across
// every account, not just the exporting one) — this format is designed so
// nothing about the database itself is observable from the file.

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct BackupBundle {
    pub schema_version: u32,
    pub exported_at: String,
    pub app_version: String,
    pub entities: Vec<BackupEntity>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct BackupEntity {
    // File-relative, 1-based, assigned at export time — not a database id.
    pub r#ref: u32,
    pub name: String,
    // Always a literal readable name (e.g. "Friend"), resolved by the
    // caller via ActiveStorage::get_entity_types() before this is built —
    // never a raw entity_types foreign-key string. See build_backup_bundle.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub entity_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub parent_ref: Option<u32>,
    // Explicit, backend-independent Self marker — replaces matching against
    // LOCAL_SELF_ENTITY_ID or SELF_ENTITY_TYPE_ID, neither of which should
    // ever appear in this file. `self` is a Rust keyword, hence the rename.
    #[serde(rename = "self", skip_serializing_if = "std::ops::Not::not", default)]
    pub self_entity: bool,
    pub drift: f64,
    pub created_at: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub moments: Vec<BackupMoment>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct BackupMoment {
    // File-relative, 1-based — flat across the WHOLE bundle, not scoped to
    // one entity's moments, since a moment can depend on one that lives
    // under a different entity.
    pub r#ref: u32,
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub gravity: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub due_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scheduled_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub until_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub completed_at: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub depends_on: Vec<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub sort_index: Option<f64>,
    #[serde(default)]
    pub created_at: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub reactions: Vec<BackupReaction>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub additional_entity_refs: Vec<u32>,
    // Momentos (2026-08-02, moment_type_id 4 / kind "momento" only) — same
    // meaning as MomentMetadata's own fields in types.rs; occurrence dates
    // here don't need ref-remapping (they're plain calendar dates, not
    // cross-references to another record in this file).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub recurrence_rule: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub momento_completed_occurrences: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub momento_excluded_occurrences: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reveal_lead: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct BackupReaction {
    // No id field — nothing else in this format references a reaction by
    // id (a reaction's moment is implicit in its nesting position), so
    // unlike entities/moments there's no cross-reference to preserve here.
    pub description: String,
    pub value: i32,
}

pub fn build_backup_bundle(
    entities: &[EntityType],
    moments: &[MomentType],
    entity_type_names: &std::collections::HashMap<String, String>,
    exported_at: String,
) -> BackupBundle {
    // Stable, deterministic ref assignment — sorted by created_at so the
    // resulting file reads in a sensible order for a human, not just
    // whatever order the storage backend happened to return.
    let mut sorted_entities: Vec<&EntityType> = entities.iter().collect();
    sorted_entities.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    let entity_ref_of: std::collections::HashMap<String, u32> = sorted_entities.iter()
        .enumerate()
        .map(|(i, e)| (e.id.clone(), (i + 1) as u32))
        .collect();

    let mut sorted_moments: Vec<&MomentType> = moments.iter().collect();
    sorted_moments.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    let moment_ref_of: std::collections::HashMap<String, u32> = sorted_moments.iter()
        .enumerate()
        .map(|(i, m)| (m.id.clone(), (i + 1) as u32))
        .collect();

    let backup_entities = sorted_entities.iter().map(|e| {
        let own_moments: Vec<&MomentType> = sorted_moments.iter()
            .filter(|m| m.entity_id == e.id)
            .copied()
            .collect();
        BackupEntity {
            r#ref: entity_ref_of[&e.id],
            name: e.name.clone(),
            entity_type: e.entity_type_id.as_ref().and_then(|id| entity_type_names.get(id).cloned()),
            // Silently dropped if the parent isn't in this export (e.g. it
            // was deleted separately) — same best-effort posture the rest
            // of this format already follows.
            parent_ref: e.parent_entity_id.as_ref().and_then(|id| entity_ref_of.get(id).copied()),
            self_entity: is_self_for_backup(e),
            drift: e.drift,
            created_at: e.created_at.clone(),
            moments: own_moments.iter().map(|m| {
                let meta = m.metadata.clone().unwrap_or_default();
                BackupMoment {
                    r#ref: moment_ref_of[&m.id],
                    kind: moment_type_str(m.moment_type_id).to_string(),
                    title: m.title.clone(),
                    description: m.description.clone().filter(|d| !d.is_empty()),
                    gravity: m.gravity,
                    due_at: m.due_at.clone(),
                    scheduled_at: meta.scheduled_at,
                    until_at: meta.until_at,
                    priority: meta.priority,
                    project: meta.project,
                    completed_at: m.completed_at.clone(),
                    depends_on: m.dependency_ids().iter().filter_map(|id| moment_ref_of.get(id).copied()).collect(),
                    tags: meta.tags,
                    sort_index: meta.sort_index,
                    created_at: m.created_at.clone(),
                    reactions: m.reactions.as_deref().unwrap_or(&[]).iter()
                        .map(|r| BackupReaction { description: r.description.clone(), value: r.value })
                        .collect(),
                    additional_entity_refs: meta.additional_entity_ids.iter().filter_map(|id| entity_ref_of.get(id).copied()).collect(),
                    recurrence_rule: meta.recurrence_rule,
                    momento_completed_occurrences: meta.momento_completed_occurrences,
                    momento_excluded_occurrences: meta.momento_excluded_occurrences,
                    reveal_lead: meta.reveal_lead,
                }
            }).collect(),
        }
    }).collect();

    BackupBundle {
        schema_version: BACKUP_SCHEMA_VERSION,
        exported_at,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        entities: backup_entities,
    }
}

pub fn render_backup(bundle: &BackupBundle) -> Result<String, VaultFormatError> {
    Ok(serde_norway::to_string(bundle)?)
}

pub fn parse_backup(yaml: &str) -> Result<BackupBundle, VaultFormatError> {
    Ok(serde_norway::from_str(yaml)?)
}

// --- vault-root files (.peeplist/vault.yaml, .peeplist/trash.yaml) --------
//
// Shapes only, per §1d's vault layout — nothing reads/writes these yet
// (that's Phase 1e, alongside the actual filesystem/localStorage backends).

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct VaultMeta {
    pub schema_version: u32,
    pub created_at: String,
    pub app_version: String,
}

// Append-only, non-destructive soft-delete log — read back and restorable
// via LocalStorage::get_deleted_moments/restore_moment (src/api/local.rs,
// src/api/local_desktop.rs), added 2026-07-22. Entities aren't restorable
// yet, only moments — TrashEntry::Entity is still write-only.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TrashEntry {
    Moment { entity_id: String, moment: MomentEntry, deleted_at: String },
    Entity { entity: EntityDoc, deleted_at: String },
}

pub fn render_vault_meta(meta: &VaultMeta) -> Result<String, VaultFormatError> {
    Ok(serde_norway::to_string(meta)?)
}

pub fn parse_vault_meta(content: &str) -> Result<VaultMeta, VaultFormatError> {
    Ok(serde_norway::from_str(content)?)
}

pub fn render_trash(entries: &[TrashEntry]) -> Result<String, VaultFormatError> {
    Ok(serde_norway::to_string(entries)?)
}

pub fn parse_trash(content: &str) -> Result<Vec<TrashEntry>, VaultFormatError> {
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    Ok(serde_norway::from_str(content)?)
}

#[cfg(test)]
mod tests {
    use super::*;


    fn sample_entity() -> EntityType {
        EntityType {
            id: "3f9a2b7e-1234-4a1b-9c3d-abcdef012345".to_string(),
            name: "Jane Doe".to_string(),
            entity_type_id: Some("Friend".to_string()),
            parent_entity_id: None,
            created_at: "2024-03-01T10:00:00Z".to_string(),
            updated_at: "2024-03-01T10:00:00Z".to_string(),
            drift: 2.0,
            metadata: Some(EntityMetadata::default()),
        }
    }

    fn task(entity_id: &str) -> MomentType {
        MomentType {
            id: "8f2c1e40-0000-0000-0000-000000000000".to_string(),
            title: "Follow up about the wedding invite".to_string(),
            description: None,
            gravity: None,
            entity_id: entity_id.to_string(),
            moment_type_id: 1,
            due_at: Some("2026-07-20".to_string()),
            completed_at: None,
            deleted_at: None,
            reactions: None,
            created_at: "2026-06-01T00:00:00Z".to_string(),
            updated_at: "2026-06-01T00:00:00Z".to_string(),
            depends_on: None,
            metadata: Some(MomentMetadata { tags: vec!["wedding".to_string()], ..Default::default() }),
        }
    }

    fn note_with_reaction(entity_id: &str) -> MomentType {
        MomentType {
            id: "77bd9a10-0000-0000-0000-000000000000".to_string(),
            title: "Ramen place".to_string(),
            description: Some("She mentioned wanting to try the new ramen place.".to_string()),
            gravity: Some(2),
            entity_id: entity_id.to_string(),
            moment_type_id: 3,
            due_at: None,
            completed_at: None,
            deleted_at: None,
            reactions: Some(vec![ReactionType {
                id: "c4e1".to_string(),
                description: "That made her day".to_string(),
                moment_id: "77bd9a10-0000-0000-0000-000000000000".to_string(),
                value: 3,
            }]),
            created_at: "2026-07-01T18:22:00Z".to_string(),
            updated_at: "2026-07-01T18:22:00Z".to_string(),
            depends_on: None,
            metadata: None,
        }
    }

    fn promise(entity_id: &str, depends_on: Option<String>) -> MomentType {
        MomentType {
            id: "5e21".to_string(),
            title: "Call her for her birthday".to_string(),
            description: None,
            gravity: None,
            entity_id: entity_id.to_string(),
            moment_type_id: 2,
            due_at: None,
            completed_at: Some("2026-05-14T09:00:00Z".to_string()),
            deleted_at: None,
            reactions: None,
            created_at: "2026-05-01T00:00:00Z".to_string(),
            updated_at: "2026-05-01T00:00:00Z".to_string(),
            depends_on,
            metadata: None,
        }
    }

    #[test]
    fn round_trips_entity_and_moments() {
        let entity = sample_entity();
        let moments = vec![task(&entity.id), note_with_reaction(&entity.id), promise(&entity.id, Some(task(&entity.id).id))];

        let rendered = render_entity_file(&entity, &moments, "");
        assert!(rendered.starts_with("---\n"));
        assert!(rendered.contains(BODY_PLACEHOLDER));

        let parsed = parse_entity_file(&rendered).expect("valid round-trip");
        assert_eq!(parsed.entity, entity);
        assert_eq!(parsed.moments.len(), moments.len());
        // A moment using the legacy single-dependency field (like `promise`
        // above, simulating pre-2026-07-29 data) round-trips with that
        // dependency migrated into metadata.depends_on instead — that's the
        // intended upgrade (see MomentType::dependency_ids), not data loss,
        // so compare the two shapes' meaning rather than raw struct equality.
        let normalize = |m: &MomentType| -> MomentType {
            let mut n = m.clone();
            let deps = n.dependency_ids();
            n.depends_on = None;
            let mut meta = n.metadata.unwrap_or_default();
            meta.depends_on = deps;
            n.metadata = if meta == MomentMetadata::default() { None } else { Some(meta) };
            n
        };
        for (original, back) in moments.iter().zip(parsed.moments.iter()) {
            assert_eq!(&normalize(original), back);
        }
        assert_eq!(parsed.body, BODY_PLACEHOLDER);
    }

    #[test]
    fn round_trips_taskwarrior_style_attributes() {
        let entity = sample_entity();
        let mut with_attrs = task(&entity.id);
        with_attrs.metadata = Some(MomentMetadata {
            tags: vec!["wedding".to_string()],
            sort_index: Some(2.0),
            priority: Some("H".to_string()),
            project: Some("Home.Garden".to_string()),
            scheduled_at: Some("2026-08-01T00:00:00Z".to_string()),
            until_at: Some("2026-09-01T00:00:00Z".to_string()),
            depends_on: vec![],
            additional_entity_ids: vec![],
            recurrence_rule: None,
            momento_completed_occurrences: vec![],
            momento_excluded_occurrences: vec![],
            reveal_lead: None,
        });

        let rendered = render_entity_file(&entity, &[with_attrs.clone()], "");
        assert!(rendered.contains("priority: H"));
        assert!(rendered.contains("project: Home.Garden"));
        assert!(rendered.contains("scheduled_at:"));
        assert!(rendered.contains("until_at:"));

        let parsed = parse_entity_file(&rendered).expect("valid round-trip");
        assert_eq!(parsed.moments[0], with_attrs);
    }

    #[test]
    fn preserves_freeform_body_on_rewrite() {
        let entity = sample_entity();
        let body = "Some personal notes about Jane.\n\nMore notes.\n";
        let rendered = render_entity_file(&entity, &[], body);
        let parsed = parse_entity_file(&rendered).expect("valid round-trip");
        assert_eq!(parsed.body, body);
    }

    #[test]
    fn omits_empty_optional_fields() {
        let entity = EntityType {
            id: "abc".to_string(),
            name: "Bare Entity".to_string(),
            entity_type_id: None,
            parent_entity_id: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            drift: 2.0,
            metadata: None,
        };
        let bare_task = MomentType {
            id: "m1".to_string(),
            title: "A bare task".to_string(),
            description: None,
            gravity: None,
            entity_id: entity.id.clone(),
            moment_type_id: 1,
            due_at: None,
            completed_at: None,
            deleted_at: None,
            reactions: None,
            created_at: "2026-01-02T00:00:00Z".to_string(),
            updated_at: "2026-01-02T00:00:00Z".to_string(),
            depends_on: None,
            metadata: None,
        };

        let rendered = render_entity_file(&entity, &[bare_task], "");
        assert!(!rendered.contains("entity_type:"));
        assert!(!rendered.contains("description:"));
        assert!(!rendered.contains("gravity:"));
        assert!(!rendered.contains("due_at:"));
        assert!(!rendered.contains("tags:"));
        assert!(!rendered.contains("reactions:"));
        assert!(!rendered.contains("priority:"));
        assert!(!rendered.contains("project:"));
        assert!(!rendered.contains("scheduled_at:"));
        assert!(!rendered.contains("until_at:"));

        let parsed = parse_entity_file(&rendered).unwrap();
        assert_eq!(parsed.entity.entity_type_id, None);
        assert_eq!(parsed.moments[0].title, "A bare task");
    }

    #[test]
    fn frontmatter_delimiter_is_not_confused_by_a_markdown_rule_in_a_description() {
        let entity = sample_entity();
        let mut noisy = task(&entity.id);
        noisy.description = Some("Section one\n---\nSection two".to_string());
        let rendered = render_entity_file(&entity, &[noisy.clone()], "notes\n");
        let parsed = parse_entity_file(&rendered).expect("should not truncate at the fake delimiter");
        assert_eq!(parsed.moments[0].description, noisy.description);
        assert_eq!(parsed.body, "notes\n");
    }

    #[test]
    fn parses_frontmatter_with_no_trailing_body_at_all() {
        let entity = sample_entity();
        let doc = entity_to_doc(&entity, &[]);
        let yaml = serde_norway::to_string(&doc).unwrap();
        let content = format!("---\n{yaml}---");
        let parsed = parse_entity_file(&content).expect("frontmatter-only file should still parse");
        assert_eq!(parsed.entity, entity);
        assert_eq!(parsed.body, "");
    }

    #[test]
    fn self_entity_uses_reserved_id() {
        let mut entity = sample_entity();
        entity.id = LOCAL_SELF_ENTITY_ID.to_string();
        let rendered = render_entity_file(&entity, &[], "");
        let parsed = parse_entity_file(&rendered).unwrap();
        assert_eq!(parsed.entity.id, LOCAL_SELF_ENTITY_ID);
    }

    #[test]
    fn filenames_are_slug_plus_short_id_and_stable_across_renames() {
        let name1 = entity_filename("Jane Doe", "3f9a2b7e-1234-4a1b-9c3d-abcdef012345");
        assert_eq!(name1, "jane-doe--3f9a2b7e.md");

        // Renaming changes the slug but not the id suffix — same id, same
        // filename stem the app should keep using (the caller's job to not
        // regenerate the filename from the new name on rename, this just
        // confirms the id portion is deterministic and slug-independent).
        let name2 = entity_filename("Jane Smith", "3f9a2b7e-1234-4a1b-9c3d-abcdef012345");
        assert!(name2.ends_with("--3f9a2b7e.md"));
    }

    #[test]
    fn backup_bundle_uses_file_relative_refs_not_raw_ids() {
        let parent = sample_entity(); // id "3f9a2b7e-..."
        let mut child = sample_entity();
        child.id = "child-uuid-0001".to_string();
        child.name = "Jane's Sister".to_string();
        child.parent_entity_id = Some(parent.id.clone());
        child.created_at = "2024-04-01T10:00:00Z".to_string();

        let t = task(&parent.id); // id "8f2c1e40-..."
        let mut linked_promise = promise(&child.id, Some(t.id.clone()));
        linked_promise.metadata = Some(MomentMetadata {
            additional_entity_ids: vec![parent.id.clone()],
            ..Default::default()
        });

        let entities = vec![parent.clone(), child.clone()];
        let moments = vec![t.clone(), linked_promise.clone()];
        let type_names: std::collections::HashMap<String, String> =
            [("Friend".to_string(), "Friend".to_string())].into_iter().collect();

        let bundle = build_backup_bundle(&entities, &moments, &type_names, "2026-08-01T00:00:00Z".to_string());

        // No raw database id appears anywhere in the bundle — every id-shaped
        // string above ("3f9a2b7e...", "child-uuid-0001", "8f2c1e40...",
        // "5e21") must be entirely absent from the rendered file.
        let rendered = render_backup(&bundle).expect("renders");
        for raw_id in [&parent.id, &child.id, &t.id, &linked_promise.id] {
            assert!(!rendered.contains(raw_id.as_str()), "raw id {raw_id} leaked into the backup file");
        }
        assert!(rendered.contains("Friend"), "entity_type should be a literal name");

        // Cross-references resolve to small relative ref numbers, not ids.
        let parent_entry = bundle.entities.iter().find(|e| e.name == "Jane Doe").unwrap();
        let child_entry = bundle.entities.iter().find(|e| e.name == "Jane's Sister").unwrap();
        assert_eq!(child_entry.parent_ref, Some(parent_entry.r#ref));
        assert_eq!(parent_entry.entity_type.as_deref(), Some("Friend"));

        let task_entry = parent_entry.moments.iter().find(|m| m.title.contains("wedding")).unwrap();
        let promise_entry = child_entry.moments.iter().find(|m| m.kind == "promise").unwrap();
        assert_eq!(promise_entry.depends_on, vec![task_entry.r#ref]);
        assert_eq!(promise_entry.additional_entity_refs, vec![parent_entry.r#ref]);

        // Round-trips through YAML with no loss.
        let parsed = parse_backup(&rendered).expect("valid round-trip");
        assert_eq!(parsed, bundle);
    }

    #[test]
    fn backup_bundle_marks_self_entity_explicitly() {
        let mut self_entity = sample_entity();
        self_entity.id = LOCAL_SELF_ENTITY_ID.to_string();
        let bundle = build_backup_bundle(&[self_entity], &[], &std::collections::HashMap::new(), "2026-08-01T00:00:00Z".to_string());
        // The only place "self" is marked is the explicit boolean flag —
        // BackupEntity has no id field at all (just a small relative ref),
        // so the Local-only sentinel id has nothing to leak through even in
        // principle, unlike the old format's doc.id == LOCAL_SELF_ENTITY_ID
        // sniffing.
        assert!(bundle.entities[0].self_entity);
        assert_eq!(bundle.entities[0].r#ref, 1);
    }
}
