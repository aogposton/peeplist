use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use super::client::SupabaseClient;
use crate::types::*;

pub async fn createEntity(entity: NewEntityType,token: String) -> Result<EntityType, reqwest::Error> {
    let response = SupabaseClient::new(token)
        .post("entities")
        .header("Prefer", "return=representation")
        .json(&entity)
        .send()
        .await?;

    let mut entities: Vec<EntityType> = response.json().await?;
    Ok(entities.remove(0))
}


pub async fn getEntities(token: String) -> Result<Vec<EntityType>, reqwest::Error> {
    let response = SupabaseClient::new(token)
        .get("entities")
        .send()
        .await?;
    
    let entities = response.json::<Vec<EntityType>>().await?;
    Ok(entities)
}


pub async fn update_entity_field(id: String, field: &str, value: Value, token: String) -> Result<(), reqwest::Error> {
    let payload = serde_json::json!({
        field: super::coerce_fk_value(field, value)
    });

    SupabaseClient::new(token)
        .patch("entities", &id)
        .json(&payload)
        .send()
        .await?;

    Ok(())
}

pub async fn getEntityTypes(token: String) -> Result<Vec<EntityTypeType>, reqwest::Error> {
    let response = SupabaseClient::new(token)
        .get("entity_types")
        .send()
        .await?;

    let result = response.json::<Vec<EntityTypeType>>().await?;
    Ok(result)
}

// No delete-entity path exists anywhere in the UI yet (see storage.rs's
// ActiveStorage::delete_entity) — this just closes the gap at the API layer.
pub async fn deleteEntity(id: String, token: String) -> Result<(), reqwest::Error> {
    SupabaseClient::new(token)
        .delete("entities", &id)
        .send()
        .await?;

    Ok(())
}
