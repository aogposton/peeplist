use serde::{Deserialize, Serialize};
use dioxus::prelude::*;
use uuid::Uuid;

// Entities
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EntityType {
    pub id: i64,
    pub name: String,
    pub entity_type_id: Option<i64>,
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
