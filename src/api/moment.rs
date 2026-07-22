use serde_json::Value;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use super::client::SupabaseClient;
use crate::types::*;


pub async fn deleteReaction(mut reaction: ReactionType,token: String) -> Result<(), reqwest::Error> {
    let response = SupabaseClient::new(token)
        .delete("reactions", &reaction.id)
        .json(&reaction)
        .send()
        .await?;

    Ok(())
}

pub async fn deleteMoment(moment: MomentType, token: String) -> Result<(), reqwest::Error> {
    // Soft delete: was PATCHing the whole `moment` struct, which includes
    // `reactions` — not a real column on the `moments` table (it's only
    // populated client-side via the `select=*,reactions(*)` embed on
    // fetch) — so PostgREST rejected every delete with an unknown-column
    // error. It also stamped `deleted_at` with `to_string()` instead of
    // `to_rfc3339()`, which isn't valid timestamptz input either. Routing
    // through the same single-field patch every other edit already uses
    // sidesteps both.
    update_moment_field(moment.id, "deleted_at", serde_json::json!(chrono::Utc::now().to_rfc3339()), token).await
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

pub async fn update_moment_field( id: String, field: &str, value: Value, token: String) -> Result<(), reqwest::Error> {
    let payload = serde_json::json!({
        field: super::coerce_fk_value(field, value)
    });

    let response = SupabaseClient::new(token)
        .patch("moments", &id)
        .json(&payload)
        .send()
        .await?;

    Ok(())
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

// Counterpart to getMoments' `deleted_at=is.null` — deleteMoment (above)
// only ever soft-deletes (sets deleted_at), so the row is still sitting in
// the same table, just filtered out of the normal query. "Recently
// deleted" reads it back with the inverse filter; restoreMoment below
// undoes it by clearing deleted_at back to null.
pub async fn getDeletedMoments(token: String) -> Result<Vec<MomentType>, reqwest::Error> {
    let response = SupabaseClient::new(token)
        .get("moments?deleted_at=not.is.null&select=*,reactions(*)&order=deleted_at.desc")
        .send()
        .await?;
    let moments = response.json::<Vec<MomentType>>().await?;
    Ok(moments)
}

pub async fn restoreMoment(id: String, token: String) -> Result<(), reqwest::Error> {
    update_moment_field(id, "deleted_at", Value::Null, token).await
}
