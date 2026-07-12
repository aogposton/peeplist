use serde_json::Value;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use super::client::SupabaseClient;
use crate::types::*;


pub async fn deleteReaction(mut reaction: ReactionType,token: String) -> Result<(), reqwest::Error> {
    let response = SupabaseClient::new(token)
        .delete("reactions", reaction.id)
        .json(&reaction)
        .send()
        .await?;

    Ok(())
}

pub async fn deleteMoment(mut moment: MomentType,token: String) -> Result<(), reqwest::Error> {
    moment.deleted_at = Some(chrono::Utc::now().to_string());

    let response = SupabaseClient::new(token)
        .patch("moments", moment.id)
        .json(&moment)
        .send()
        .await?;

    Ok(())
}

pub async fn createMoment(moment: NewMomentType,token: String) -> Result<MomentType, reqwest::Error> {
    let response = SupabaseClient::new(token)
        .post("moments")
        .header("Prefer", "return=representation")
        .json(&moment)
        .send()
        .await?;

    let mut moments: Vec<MomentType> = response.json().await?;
    Ok(moments.remove(0))
}

pub async fn createReaction(reaction: NewReactionType,token: String) -> Result<ReactionType, reqwest::Error> {
    let response = SupabaseClient::new(token)
        .post("reactions")
        .header("Prefer", "return=representation")
        .json(&reaction)
        .send()
        .await?;
    let created = response.json::<Vec<ReactionType>>().await?;
    Ok(created.into_iter().next().unwrap())
}

pub async fn update_moment_field( id: i64, field: &str, value: Value, token: String) -> Result<(), reqwest::Error> {
    let payload = serde_json::json!({
        field: value
    });

    let response = SupabaseClient::new(token)
        .patch("moments", id)
        .json(&payload)
        .send()
        .await?;

    Ok(())
}

pub async fn updateMoment(moment: MomentType, token: String) -> Result<MomentType, reqwest::Error> {
    let response = SupabaseClient::new(token)
        .patch("moments", moment.id)
        .header("Prefer", "return=representation")
        .json(&moment)
        .send()
        .await?;

    let mut moments: Vec<MomentType> = response.json().await?;
    Ok(moments.remove(0))
}

pub async fn getMoments(token: String) -> Result<Vec<MomentType>, reqwest::Error> {
    // Without an explicit order, Postgres/PostgREST returns rows in unspecified
    // (effectively random-looking) order. id.asc gives a stable baseline; the
    // client layers Default/Due date/Custom sort modes on top of this.
    let response = SupabaseClient::new(token)
        .get("moments?deleted_at=is.null&select=*,reactions(*)&order=id.asc")
        .send()
        .await?;
    let moments = response.json::<Vec<MomentType>>().await?;
    Ok(moments)
}
