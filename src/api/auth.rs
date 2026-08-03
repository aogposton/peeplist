
use reqwest::{Client, RequestBuilder};
use std::env;
use super::client::SupabaseClient;
use crate::types::*;

// Distinguishes "never reached the server" from "the server said no" — the
// two session-liveness checks in layouts/navbar.rs (the mount-time check
// and the 50-minute proactive refresh loop) used to collapse both into a
// plain String and treat any failure as "token is dead, log out." That
// meant going offline and refreshing the page reliably logged the Synced
// vault out from under you: the validity check AND its refresh fallback
// both fail identically when there's no network to reach at all, with
// nothing distinguishing that from an actually-expired/revoked token.
// Mirrors StorageError::Network vs StorageError::Remote (api/storage.rs).
#[derive(Debug)]
pub enum AuthError {
    Network(reqwest::Error),
    Rejected(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::Network(e) => write!(f, "{e}"),
            AuthError::Rejected(msg) => write!(f, "{msg}"),
        }
    }
}

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

// Folds an optional Cloudflare Turnstile token into a request body as
// `gotrue_meta_security.captcha_token` — the shape Supabase's own
// signup/login/recover endpoints expect once "Enable CAPTCHA protection"
// is turned on in that project's Auth settings. Omitted entirely (not just
// null) when there's no token, so this is a no-op until that setting is
// actually turned on — see views/auth.rs's Turnstile widget wiring.
fn with_captcha(mut body: serde_json::Value, captcha_token: Option<String>) -> serde_json::Value {
    if let Some(token) = captcha_token {
        body["gotrue_meta_security"] = serde_json::json!({ "captcha_token": token });
    }
    body
}

pub async fn signup(email: String, password: String, captcha_token: Option<String>) -> Result<SignupOutcome, String> {
    let body = with_captcha(serde_json::json!({ "email": email, "password": password }), captcha_token);
    let response = SupabaseClient::new("".to_string())
        .auth_post("signup")
        .json(&body)
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

pub async fn login(email: String, password: String, captcha_token: Option<String>) -> Result<LoginResponse, String> {
    let body = with_captcha(serde_json::json!({ "email": email, "password": password }), captcha_token);
    let response = SupabaseClient::new("".to_string())
        .auth_post("token?grant_type=password")
        .json(&body)
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
/// without the user explicitly logging out. See AuthError's own doc
/// comment for why the error type distinguishes offline from rejected.
pub async fn get_current_user(token: String) -> Result<AuthUser, AuthError> {
    let response = SupabaseClient::new(token)
        .auth_get("user")
        .send()
        .await
        .map_err(AuthError::Network)?;

    if !response.status().is_success() {
        return Err(AuthError::Rejected(format!("token rejected with status {}", response.status())));
    }

    response.json::<AuthUser>().await.map_err(AuthError::Network)
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

/// Sends a Supabase password-recovery email. `redirect_to` is where the
/// emailed link lands (must be in the Supabase project's Auth > URL
/// Configuration allow-list) — Supabase appends the actual recovery tokens
/// as a URL *fragment* on that link (`#access_token=...&type=recovery`),
/// which `ResetPasswordCmp` (views/auth.rs) reads and feeds into
/// `update_password` above to actually set the new password. Always
/// returns Ok on a well-formed request regardless of whether the email is
/// registered — that's Supabase's own anti-enumeration behavior, not
/// something to work around.
pub async fn request_password_reset(email: String, redirect_to: String, captcha_token: Option<String>) -> Result<(), String> {
    // Manual percent-encoding rather than reqwest's `.query()` builder (not
    // available on this reqwest version's RequestBuilder) — redirect_to is
    // a full URL (scheme, host, path), which needs its reserved characters
    // encoded to sit safely inside another URL's query string.
    let encoded_redirect: String = redirect_to.bytes().map(|b| match b {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
        _ => format!("%{:02X}", b),
    }).collect();
    let body = with_captcha(serde_json::json!({ "email": email }), captcha_token);
    let response = SupabaseClient::new("".to_string())
        .auth_post(&format!("recover?redirect_to={encoded_redirect}"))
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(format!("Password reset request failed ({}): {}", status, text));
    }
    Ok(())
}

pub async fn refresh_access_token(refresh_token: String) -> Result<LoginResponse, AuthError> {
    let response = SupabaseClient::new("".to_string())
        .auth_post("token?grant_type=refresh_token")
        .json(&serde_json::json!({ "refresh_token": refresh_token }))
        .send()
        .await
        .map_err(AuthError::Network)?;

    let status = response.status();
    let text = response.text().await.map_err(AuthError::Network)?;

    if !status.is_success() {
        return Err(AuthError::Rejected(format!("Refresh failed ({}): {}", status, text)));
    }

    serde_json::from_str::<LoginResponse>(&text)
        .map_err(|e| AuthError::Rejected(format!("Failed to parse refresh response: {} — body was: {}", e, text)))
}
