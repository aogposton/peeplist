
use reqwest::{Client, RequestBuilder};
use std::env;
use super::client::SupabaseClient;
use crate::types::*;

//pub async fn login(email: String, password: String) -> Result<LoginResponse, reqwest::Error> {
//    SupabaseClient::new("".to_string())
//        .auth_post("token?grant_type=password")
//        .json(&LoginRequest {
//            email,
//            password,
//        })
//        .send()
//        .await?
//        .json::<LoginResponse>()
//        .await
//}

pub async fn login(email: String, password: String) -> Result<LoginResponse, String> {
    let response = SupabaseClient::new("".to_string())
        .auth_post("token?grant_type=password")
        .json(&LoginRequest { email, password })
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = response.status();
    let text = response.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        // This will show you the REAL reason: bad password, unconfirmed email, wrong project, etc.
        return Err(format!("Login failed ({}): {}", status, text));
    }

    serde_json::from_str::<LoginResponse>(&text)
        .map_err(|e| format!("Failed to parse login response: {} — body was: {}", e, text))
}
