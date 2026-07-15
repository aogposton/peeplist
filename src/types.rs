use serde::{Deserialize, Serialize};
use dioxus::prelude::*;
use uuid::Uuid;

fn default_drift() -> f64 { 2.0 }

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
    pub id: i64,
    pub name: String,
    pub entity_type_id: Option<i64>,
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
    pub id: i64,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct NewEntityType {
    pub name: String,
    pub entity_type_id: Option<i64>,
    pub parent_entity_id: Option<i64>,
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
    pub id: i64,
    pub title: String,
    pub description: Option<String>,
    pub gravity: Option<i32>,
    pub entity_id: i64,
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
    #[serde(default)]
    pub depends_on: Option<i64>,
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
    pub id: i64,
    pub description: String,
    pub moment_id: i64,
    pub value: i32,
}


#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct NewReactionType {
    pub description: String,
    pub moment_id: i64,
    pub value: i32,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct NewMomentType {
    pub title: String,
    pub description: Option<String>,
    pub gravity: Option<i32>,
    pub entity_id: i64,
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
