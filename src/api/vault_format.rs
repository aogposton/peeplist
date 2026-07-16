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

use crate::types::{EntityMetadata, EntityType, MomentMetadata, MomentType, ReactionType};
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
    #[serde(default = "default_drift")]
    pub drift: f64,
    #[serde(default)]
    pub created_at: String,
    #[serde(skip_serializing_if = "str::is_empty", default)]
    pub relationship: String,
    #[serde(skip_serializing_if = "str::is_empty", default)]
    pub how_met: String,
    #[serde(skip_serializing_if = "str::is_empty", default)]
    pub birthday: String,
    #[serde(skip_serializing_if = "str::is_empty", default)]
    pub location: String,
    #[serde(skip_serializing_if = "str::is_empty", default)]
    pub why: String,
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
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub depends_on: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub sort_index: Option<f64>,
    #[serde(default)]
    pub created_at: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub reactions: Vec<ReactionEntry>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ReactionEntry {
    pub id: String,
    pub description: String,
    pub value: i32,
}

// --- moment_type_id <-> "task"/"promise"/"note" -------------------------
// Mirrors the mapping already implemented as `kind_label` in
// src/components/entity.rs — kept in sync by hand since that one renders
// for display ("Task"/"Promise"/"Note") and this one is a wire format key
// ("task"/"promise"/"note"), not worth sharing a single function over.

fn moment_type_str(moment_type_id: i64) -> &'static str {
    match moment_type_id {
        2 => "promise",
        3 => "note",
        _ => "task",
    }
}

fn moment_type_id(kind: &str) -> i64 {
    match kind {
        "promise" => 2,
        "note" => 3,
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
        depends_on: m.depends_on.clone(),
        tags: meta.tags,
        sort_index: meta.sort_index,
        created_at: m.created_at.clone(),
        reactions: m.reactions.as_deref().unwrap_or(&[]).iter().map(reaction_to_entry).collect(),
    }
}

// Soft-deleted moments never appear in the visible file (see §1d — they're
// filtered out and appended to trash.yaml instead), so `deleted_at` is
// always None for anything round-tripped through this format.
fn entry_to_moment(entry: &MomentEntry, entity_id: &str) -> MomentType {
    let meta = MomentMetadata {
        tags: entry.tags.clone(),
        sort_index: entry.sort_index,
        priority: entry.priority.clone(),
        project: entry.project.clone(),
        scheduled_at: entry.scheduled_at.clone(),
        until_at: entry.until_at.clone(),
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
        depends_on: entry.depends_on.clone(),
        metadata,
    }
}

pub(crate) fn entity_to_doc(entity: &EntityType, moments: &[MomentType]) -> EntityDoc {
    let meta = entity.metadata.clone().unwrap_or_default();
    EntityDoc {
        id: entity.id.clone(),
        name: entity.name.clone(),
        entity_type: entity.entity_type_id.clone(),
        drift: entity.drift,
        created_at: entity.created_at.clone(),
        relationship: meta.relationship,
        how_met: meta.how_met,
        birthday: meta.birthday,
        location: meta.location,
        why: meta.why,
        moments: moments.iter().map(moment_to_entry).collect(),
    }
}

fn doc_to_entity(doc: &EntityDoc) -> (EntityType, Vec<MomentType>) {
    let metadata = EntityMetadata {
        relationship: doc.relationship.clone(),
        how_met: doc.how_met.clone(),
        birthday: doc.birthday.clone(),
        location: doc.location.clone(),
        why: doc.why.clone(),
    };
    let entity = EntityType {
        id: doc.id.clone(),
        name: doc.name.clone(),
        entity_type_id: doc.entity_type.clone(),
        created_at: doc.created_at.clone(),
        drift: doc.drift,
        metadata: Some(metadata),
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

// Append-only, non-destructive soft-delete log — no recovery UI planned yet
// (see §1d), this just gives deleted records somewhere to go instead of
// vanishing outright.
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
            created_at: "2024-03-01T10:00:00Z".to_string(),
            drift: 2.0,
            metadata: Some(EntityMetadata {
                relationship: "Close friend".to_string(),
                how_met: "College".to_string(),
                birthday: "1990-05-14".to_string(),
                location: "Seattle, WA".to_string(),
                why: "She always shows up when it matters.".to_string(),
            }),
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
        for (original, back) in moments.iter().zip(parsed.moments.iter()) {
            assert_eq!(original, back);
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
            created_at: "2026-01-01T00:00:00Z".to_string(),
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
            depends_on: None,
            metadata: None,
        };

        let rendered = render_entity_file(&entity, &[bare_task], "");
        assert!(!rendered.contains("entity_type:"));
        assert!(!rendered.contains("relationship:"));
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
}
