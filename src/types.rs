use serde::{Deserialize, Serialize};
use dioxus::prelude::*;
use uuid::Uuid;

fn default_drift() -> f64 { 2.0 }

// Entity id 0 is reserved to always mean "yourself" (see memory
// project_self_entity_convention). IDs became UUID strings in the local-first
// migration, but the live Supabase project still assigns this row the
// integer id 0 — the value here has to match what the SupabaseStorage
// boundary stringifies that row's id to (see api/client.rs), not the plan's
// eventual "self" sentinel, which only applies once a local vault (with its
// own self.md file) exists.
pub const SELF_ENTITY_ID: &str = "0";

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
    // Server-generated on insert; never sent back on writes.
    #[serde(skip_serializing, default)]
    pub created_at: String,
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
pub struct EntityForm {
    pub name: String,
    pub relationship: String,
    pub meeting: String,
    pub bday: String,
    pub location: String,
    pub why: String,
    pub entity_type_sel: String,
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
    // deleteMoment/updateMoment PATCH this whole struct as the request body
    // (not just the changed field), so id/entity_id/depends_on need both
    // directions of the flex conversion, unlike the New*Type structs above.
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
    // Single taskwarrior-style dependency. The `moments` table only has room
    // for one (bare `depends_on bigint`, no join table) — a real multi-dependency
    // feature would need a `moment_dependencies` join table added later.
    #[serde(deserialize_with = "de_flex_id_opt", serialize_with = "se_flex_id_opt", default)]
    pub depends_on: Option<String>,
    // Freeform jsonb column, repurposed client-side for tags + manual sort
    // order rather than adding new schema. See MomentMetadata.
    #[serde(default)]
    pub metadata: Option<MomentMetadata>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct MomentMetadata {
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub sort_index: Option<f64>,
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
    pub oncontextmenu: EventHandler<MouseEvent>,
}

#[derive(Props, Clone, PartialEq)]
pub struct MomentListProps {
    pub moments: Vec<MomentType>,
}


#[derive(Props, Clone, PartialEq)]
pub struct CheckboxProps {
    pub checked: bool,
    pub on_change: EventHandler<bool>,
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
