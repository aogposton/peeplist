use reqwest::Client;
use serde::{Deserialize, Serialize};
use super::client::SupabaseClient;
use crate::types::*;

pub async fn login(email: String, password: String,) -> Result<LoginResponse, reqwest::Error> {
    Client::new()
        .post(format!(
            "{}/auth/v1/token?grant_type=password",
            self.url
        ))
        .header("apikey",self.anon_key)
        .json(&LoginRequest {
            email,
            password,
        })
        .send()
        .await?
        .json::<LoginResponse>()
        .await
}
