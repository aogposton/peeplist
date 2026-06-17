use reqwest::Client;
use serde::{Deserialize, Serialize};
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


pub async fn getEntityTypes(token: String) -> Result<Vec<EntityTypeType>, reqwest::Error> {
    let response = SupabaseClient::new(token)
        .get("entity_types")
        .send()
        .await?;
    
    let result = response.json::<Vec<EntityTypeType>>().await?;
    Ok(result)
}
