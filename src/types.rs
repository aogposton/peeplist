use serde::{Deserialize, Serialize};
use dioxus::prelude::*;
use uuid::Uuid;

fn default_drift() -> f64 { 2.0 }

// entity_types row 0 is reserved to mean "this is the type of the entity
// that IS its owner" — every Synced account gets exactly one entities row
// of this type, auto-provisioned server-side on signup (see
// scripts/2026-07-22-rls-and-self-entity.sql). Replaces the old
// entities.id == "0" convention: that was a single row shared by every
// account (a real data-isolation bug, fixed alongside RLS in the same
// migration), whereas this is a type marker each user's own row can carry.
// Not offered as a selectable type in the New Entity modal (see
// components::entity::EntityModalCmp) — it's a reserved system marker, not
// a real relationship type.
pub const SELF_ENTITY_TYPE_ID: &str = "0";

// Supabase's `bigint` id columns are unchanged (see api/client.rs) — these
// helpers are the one place that reconciles that wire shape (JSON numbers)
// with the app's UUID-ready `String` id fields, so every other file can just
// treat ids as strings. Falls back to passing strings through untouched,
// which is what a real UUID (once the local vault lands) will look like.
fn de_flex_id<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum IdRepr {
        Num(i64),
        Str(String),
    }
    Ok(match IdRepr::deserialize(deserializer)? {
        IdRepr::Num(n) => n.to_string(),
        IdRepr::Str(s) => s,
    })
}

fn de_flex_id_opt<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum IdRepr {
        Num(i64),
        Str(String),
    }
    Ok(match Option::<IdRepr>::deserialize(deserializer)? {
        Some(IdRepr::Num(n)) => Some(n.to_string()),
        Some(IdRepr::Str(s)) => Some(s),
        None => None,
    })
}

fn se_flex_id<S>(id: &str, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match id.parse::<i64>() {
        Ok(n) => serializer.serialize_i64(n),
        Err(_) => serializer.serialize_str(id),
    }
}

fn se_flex_id_opt<S>(id: &Option<String>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match id {
        Some(s) => match s.parse::<i64>() {
            Ok(n) => serializer.serialize_some(&n),
            Err(_) => serializer.serialize_some(s),
        },
        None => serializer.serialize_none(),
    }
}

// Freeform per-entity details collected in the "New Entity" modal. Stored in
// entities.metadata (jsonb) — mirrors the same pattern used for
// moments.metadata (tags/sort_index). Surfaced read-only in the Info panel.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct EntityMetadata {
    #[serde(default)]
    pub relationship: String,
    #[serde(default)]
    pub how_met: String,
    #[serde(default)]
    pub birthday: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub why: String,
}

// Entities
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EntityType {
    #[serde(deserialize_with = "de_flex_id")]
    pub id: String,
    pub name: String,
    #[serde(deserialize_with = "de_flex_id_opt", default)]
    pub entity_type_id: Option<String>,
    // Set once at individuation time (see components::sidebar::entity_list_cmp's
    // "Individuate" action) — the group entity this one was split out of.
    // Was write-only via NewEntityType until 2026-07-22 (sent on creation
    // but never read back here), so every individuated entity's provenance
    // was silently unrecoverable despite the column existing on both
    // storage backends.
    #[serde(deserialize_with = "de_flex_id_opt", default)]
    pub parent_entity_id: Option<String>,
    // Server-generated on insert; never sent back on writes.
    #[serde(skip_serializing, default)]
    pub created_at: String,
    // Server-generated/updated via a DB trigger (scripts/2026-08-02-
    // updated-at-lww.sql), never sent back on writes — used by the offline-
    // first sync flush loop (layouts/navbar.rs) to detect whether this
    // record changed on the server since a queued offline edit was staged,
    // for real last-write-wins conflict resolution. Empty string on a
    // record that predates the migration or came from the Local vault
    // (which has no LWW concept at all, single-device by definition).
    #[serde(skip_serializing, default)]
    pub updated_at: String,
    // Days per +1 unit of distance from inactivity (see Distance/Drift spec).
    // Defaults to 2.0 client-side so this degrades gracefully before the
    // `entities.drift` column exists in the DB.
    #[serde(default = "default_drift")]
    pub drift: f64,
    // Degrades gracefully (None) before entities.metadata exists in the DB.
    #[serde(default)]
    pub metadata: Option<EntityMetadata>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EntityTypeType {
    #[serde(deserialize_with = "de_flex_id")]
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct NewEntityType {
    pub name: String,
    #[serde(serialize_with = "se_flex_id_opt")]
    pub entity_type_id: Option<String>,
    #[serde(serialize_with = "se_flex_id_opt")]
    pub parent_entity_id: Option<String>,
    pub user_id: Option<Uuid>,
    pub archived_at: Option<chrono::DateTime<chrono::Utc>>,
    pub metadata: Option<EntityMetadata>,
}

#[derive(Clone, Default)]
pub struct ReactionForm {
    pub description: String,
    pub value: i32,
}

#[derive(Clone, Default)]
pub struct MomentForm {
    pub title: String,
    pub description: String,
    pub entity_sel: String,
}

// Moments
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MomentType {
    // deleteMoment PATCHes this whole struct as the request body (not just
    // the changed field), so id/entity_id/depends_on need both directions of
    // the flex conversion, unlike the New*Type structs above.
    #[serde(deserialize_with = "de_flex_id", serialize_with = "se_flex_id")]
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub gravity: Option<i32>,
    #[serde(deserialize_with = "de_flex_id", serialize_with = "se_flex_id")]
    pub entity_id: String,
    pub moment_type_id: i64,
    pub due_at: Option<String>,
    pub completed_at: Option<String>,
    pub deleted_at: Option<String>,
    pub reactions: Option<Vec<ReactionType>>,
    // Server-generated on insert; never sent back on writes (see below).
    #[serde(skip_serializing, default)]
    pub created_at: String,
    // See EntityType::updated_at's doc comment — same trigger-driven LWW
    // timestamp, same reasoning.
    #[serde(skip_serializing, default)]
    pub updated_at: String,
    // Legacy single taskwarrior-style dependency column (bare `depends_on
    // bigint`, no join table) — superseded 2026-07-29 by the unlimited
    // `metadata.depends_on` list (see MomentMetadata), same trick as
    // tags/priority/etc riding the jsonb blob instead of a schema change.
    // Still read (see MomentType::dependency_ids) for moments dependency-
    // tagged before that migration; never written to again.
    #[serde(deserialize_with = "de_flex_id_opt", serialize_with = "se_flex_id_opt", default)]
    pub depends_on: Option<String>,
    // Freeform jsonb column, repurposed client-side for tags + manual sort
    // order rather than adding new schema. See MomentMetadata.
    #[serde(default)]
    pub metadata: Option<MomentMetadata>,
}

impl MomentType {
    // Canonical accessor for taskwarrior-style dependencies now that a
    // moment can depend on more than one thing (2026-07-29) — this moment
    // is blocked until every one of these completes. `metadata.depends_on`
    // is authoritative once set; the legacy top-level `depends_on` column
    // is folded in only as a fallback for moments never touched since this
    // migration (metadata.depends_on still empty for them).
    pub fn dependency_ids(&self) -> Vec<String> {
        let from_metadata = self.metadata.as_ref().map(|m| m.depends_on.clone()).unwrap_or_default();
        if !from_metadata.is_empty() {
            return from_metadata;
        }
        self.depends_on.clone().into_iter().collect()
    }

    // Every entity this moment counts for — the primary entity_id plus any
    // additional_entity_ids (2026-07-29 multi-entity decision: additional
    // entities are full peers, not lightweight cc's — this moment closes
    // their Distance and factors into their urgency exactly as if it were
    // solely theirs). Used everywhere a plain `entity_id == X` equality
    // check used to gate whether a moment belongs to an entity.
    pub fn entity_ids(&self) -> Vec<String> {
        let mut ids = vec![self.entity_id.clone()];
        if let Some(meta) = &self.metadata {
            for id in &meta.additional_entity_ids {
                if !ids.contains(id) {
                    ids.push(id.clone());
                }
            }
        }
        ids
    }

    pub fn involves_entity(&self, entity_id: &str) -> bool {
        self.entity_id == entity_id
            || self.metadata.as_ref().is_some_and(|m| m.additional_entity_ids.iter().any(|id| id == entity_id))
    }
}

// Taskwarrior-style attributes, part 2 (see DESIGN_PROGRESS.md — the user
// wants "everything taskwarrior has"). Deliberately not new MomentType/DB
// columns: like tags/sort_index below, these ride the existing metadata
// jsonb blob, so no Supabase schema migration is needed. Real enforcement of
// `scheduled`/`until` (taskwarrior hides tasks before their scheduled date
// and auto-deletes them after `until`) is still out of scope — these fields
// are storable and editable, not yet acted on. `recur` — explicitly deferred
// when this comment was first written — is `recurrence_rule` below,
// 2026-08-02, for moment_type_id 4 ("momento") specifically.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct MomentMetadata {
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub sort_index: Option<f64>,
    // "H" / "M" / "L", taskwarrior's own convention — not an enum, so an
    // unrecognized value round-trips harmlessly instead of failing to parse.
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub scheduled_at: Option<String>,
    #[serde(default)]
    pub until_at: Option<String>,
    // Taskwarrior-style dependencies, unlimited like tags (2026-07-29) — see
    // MomentType::dependency_ids for how this combines with the legacy
    // single-dependency `depends_on` column below.
    #[serde(default)]
    pub depends_on: Vec<String>,
    // Multi-entity moments (2026-07-29): a moment can affect more than one
    // relationship at once (e.g. meeting Alle about a zine also involves
    // the zine's audience entity). These are full peers of the primary
    // entity_id, not lightweight tag-alongs — see MomentType::entity_ids.
    #[serde(default)]
    pub additional_entity_ids: Vec<String>,
    // Momentos (2026-08-02, moment_type_id 4 only) — an RFC 5545 RRULE
    // string (e.g. "FREQ=WEEKLY;BYDAY=MO"), expanded on the fly by
    // src/momento.rs wherever a momento's upcoming occurrences need
    // showing. The moment's own `due_at` doubles as the RRULE's DTSTART
    // (anchor date) — no separate anchor field needed. Occurrences are
    // never materialized as real rows; the two lists below are the only
    // per-occurrence state that exists, keyed by occurrence date
    // ("YYYY-MM-DD").
    #[serde(default)]
    pub recurrence_rule: Option<String>,
    // Occurrence dates marked done — a momento's own version of
    // completed_at, since the template itself is never "completed" (it
    // recurs forever until deleted).
    #[serde(default)]
    pub momento_completed_occurrences: Vec<String>,
    // Occurrence dates permanently excluded from the pattern (iCal's own
    // EXDATE concept) — "skip" and "delete a single occurrence" are the
    // same action, per an explicit product decision (2026-08-02): there's
    // no separate "temporarily hide, comes back later" state.
    #[serde(default)]
    pub momento_excluded_occurrences: Vec<String>,
    // How far in advance a momento surfaces on the cross-entity Momentos
    // sidebar view (2026-08-03) — one of "1hour"/"1day"/"1week"/"1month", or
    // None to always show every future occurrence (today's default
    // behavior, unchanged). Purely a display filter — see
    // src/momento.rs::is_revealed. Doesn't affect the per-entity Momentos
    // tab, which always shows everything for that entity regardless (you
    // navigated there on purpose; there's nothing to declutter).
    #[serde(default)]
    pub reveal_lead: Option<String>,
}


#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ReactionType {
    #[serde(deserialize_with = "de_flex_id", serialize_with = "se_flex_id")]
    pub id: String,
    pub description: String,
    #[serde(deserialize_with = "de_flex_id", serialize_with = "se_flex_id")]
    pub moment_id: String,
    pub value: i32,
}


#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct NewReactionType {
    pub description: String,
    #[serde(serialize_with = "se_flex_id")]
    pub moment_id: String,
    pub value: i32,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct NewMomentType {
    pub title: String,
    pub description: Option<String>,
    pub gravity: Option<i32>,
    #[serde(serialize_with = "se_flex_id")]
    pub entity_id: String,
    pub moment_type_id: i64,
    pub deleted_at: Option<String>
}

#[derive(Props, Clone, PartialEq)]
pub struct MomentCmpProps {
    pub moment: MomentType,
    pub is_note: Option<bool>,
}

#[derive(Props, Clone, PartialEq)]
pub struct MomentListProps {
    pub moments: Vec<MomentType>,
}


#[derive(Props, Clone, PartialEq)]
pub struct CheckboxProps {
    pub checked: bool,
    pub on_change: EventHandler<bool>,
    // e.g. a moment blocked by an incomplete dependency — see MomentType::depends_on.
    #[props(default)]
    pub disabled: bool,
}


#[derive(Clone, Default)]
pub struct LoginForm {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub user: AuthUser,
}

#[derive(Deserialize, Clone, Debug)]
pub struct AuthUser {
    pub id: String,
    pub email: String,
}

#[derive(Serialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}
