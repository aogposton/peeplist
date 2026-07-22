
use reqwest::{Client, RequestBuilder};
use std::env;
use super::client::SupabaseClient;
use crate::types::*;

// Self-service account creation — closes the gap flagged in the local-first
// pivot plan (Phase 1f, deliberately deferred until now): before this,
// only an account created by hand directly in Supabase could ever log in.
// Supabase's own /auth/v1/signup returns a full session (same shape as
// login) when the project auto-confirms email, or just a bare user object
// with no tokens when email confirmation is required — SignupOutcome
// distinguishes the two so the caller can log straight in or tell the user
// to check their inbox, without needing to know which mode this project is
// configured in ahead of time.
pub enum SignupOutcome {
    LoggedIn(LoginResponse),
    NeedsConfirmation,
}

pub async fn signup(email: String, password: String) -> Result<SignupOutcome, String> {
    let response = SupabaseClient::new("".to_string())
        .auth_post("signup")
        .json(&LoginRequest { email, password })
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = response.status();
    let text = response.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(format!("Signup failed ({}): {}", status, text));
    }

    match serde_json::from_str::<LoginResponse>(&text) {
        Ok(session) => Ok(SignupOutcome::LoggedIn(session)),
        Err(_) => Ok(SignupOutcome::NeedsConfirmation),
    }
}

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

/// Changes the password on the currently-authenticated Supabase user.
/// Needs the user's own access token (Supabase's /auth/v1/user PUT
/// endpoint rejects the anon key), so this can only ever apply to the
/// Synced vault, not Local (which has no Supabase account behind it).
pub async fn update_password(token: String, new_password: String) -> Result<(), String> {
    let response = SupabaseClient::new(token)
        .auth_put("user")
        .json(&serde_json::json!({ "password": new_password }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(format!("Password update failed ({}): {}", status, text));
    }
    Ok(())
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
