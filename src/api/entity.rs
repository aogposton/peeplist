use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use super::client::SupabaseClient;
use super::storage::StorageError;
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

// See moment::getMomentById's doc comment — same purpose (the offline-first
// sync LWW conflict check in layouts/navbar.rs's flush loop), same shape,
// same fix (StorageError::Remote lets the flush loop give up on a
// permanently-rejected id instead of retrying it forever).
pub async fn getEntityById(id: String, token: String) -> Result<Option<EntityType>, StorageError> {
    let response = SupabaseClient::new(token)
        .get(&format!("entities?id=eq.{id}"))
        .send()
        .await
        .map_err(StorageError::Network)?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(StorageError::Remote(format!("getEntityById failed ({}): {}", status, text)));
    }
    let mut entities: Vec<EntityType> = response.json().await.map_err(StorageError::Network)?;
    Ok(entities.pop())
}

pub async fn getEntityTypes(token: String) -> Result<Vec<EntityTypeType>, reqwest::Error> {
    let response = SupabaseClient::new(token)
        .get("entity_types")
        .send()
        .await?;

    let result = response.json::<Vec<EntityTypeType>>().await?;
    Ok(result)
}

// Was `Result<(), reqwest::Error>` with no status check — reqwest's `?`
// only errors on transport-level failure (DNS, connection refused), not on
// the server returning a non-2xx response. A DELETE rejected by Postgres
// (almost certainly a foreign-key violation here — moments.entity_id
// references this row, and nothing cascades or nulls it out) silently
// looked like success: the entity vanished from the UI, then came right
// back on the next fetch since it was never actually deleted server-side.
//
// Returns StorageError (not a plain String) specifically so the offline-
// sync flush loop (layouts/navbar.rs) can tell "never reached the server,
// retry later" (Network) apart from "server said no, permanently drop"
// (Remote) — collapsing both into one error type here would leave the
// flush loop unable to make that call for entity deletes specifically,
// even though every other queued op already distinguishes them.
pub async fn deleteEntity(id: String, token: String) -> Result<(), StorageError> {
    let response = SupabaseClient::new(token)
        .delete("entities", &id)
        .send()
        .await
        .map_err(StorageError::Network)?;

    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(StorageError::Remote(format!("Delete failed ({}): {}", status, text)));
    }
    Ok(())
}
