
use reqwest::{Client, RequestBuilder};
use std::env;
use super::client::SupabaseClient;
use crate::types::*;

pub async fn login(email: String, password: String) -> Result<LoginResponse, reqwest::Error> {
    SupabaseClient::new("".to_string())
        .auth_post("token?grant_type=password")
        .json(&LoginRequest {
            email,
            password,
        })
        .send()
        .await?
        .json::<LoginResponse>()
        .await
}
