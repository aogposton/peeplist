
use reqwest::{Client, RequestBuilder};
use std::env;
use super::client::SupabaseClient;
use crate::types::*;

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

/// Checks whether an access token is still accepted by Supabase.
/// Used on app load to detect a token that has expired/died server-side
/// without the user explicitly logging out.
pub async fn get_current_user(token: String) -> Result<AuthUser, String> {
    let response = SupabaseClient::new(token)
        .auth_get("user")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err(format!("token rejected with status {}", response.status()));
    }

    response.json::<AuthUser>().await.map_err(|e| e.to_string())
}

pub async fn refresh_access_token(refresh_token: String) -> Result<LoginResponse, String> {
    let response = SupabaseClient::new("".to_string())
        .auth_post("token?grant_type=refresh_token")
        .json(&serde_json::json!({ "refresh_token": refresh_token }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = response.status();
    let text = response.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(format!("Refresh failed ({}): {}", status, text));
    }

    serde_json::from_str::<LoginResponse>(&text)
        .map_err(|e| format!("Failed to parse refresh response: {} — body was: {}", e, text))
}
