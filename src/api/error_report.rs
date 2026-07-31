use super::client::SupabaseClient;

// Minimal crash/error visibility (2026-07-29, see scripts/2026-07-29-error-
// reports.sql) — a write-only mailbox so bugs are visible somewhere other
// than one user's own devtools console. Uses the current session's token if
// logged in (so the row's user_id gets stamped, see the SQL migration's
// trigger) or the anon key otherwise — this has to work for logged-out
// Local-vault-only usage too, which is most usage.
pub async fn report_error(token: Option<String>, message: String, context: Option<String>, user_agent: Option<String>) {
    let client = match token {
        Some(t) => SupabaseClient::new(t),
        None => SupabaseClient::anon(),
    };
    let body = serde_json::json!({
        "message": message,
        "context": context,
        "user_agent": user_agent,
    });
    // Best-effort: nowhere useful to surface a failed error *report* to, so
    // this swallows its own result rather than returning a Result nearly
    // every call site would just ignore anyway.
    let _ = client.post("error_reports").json(&body).send().await;
}
