use reqwest::{Client, RequestBuilder};
use std::env;
use crate::types::*;

pub struct SupabaseClient {
    client: Client,
    token: String,
    url: String,
    anon_key: String,
}

impl SupabaseClient {
    pub fn new(token: String) -> Self {
        let url = env!("SUPABASE_PUBLIC_URL").to_string();
        let anon_key = env!("SUPABASE_ANON_KEY").to_string();

        Self {
            client: Client::new(),
            anon_key,
            url,
            token,
        }
    }

    // For requests that need to work whether or not anyone's logged in
    // (currently just error_reports, since most usage is the Local vault
    // with no account at all) — the anon key itself is a valid, signed JWT
    // with role "anon", so using it as the bearer token (not an empty
    // string) is what makes PostgREST resolve the request to the `anon`
    // role instead of failing auth outright.
    pub fn anon() -> Self {
        let anon_key = env!("SUPABASE_ANON_KEY").to_string();
        Self::new(anon_key)
    }

    pub fn post(&self, table: &str) -> RequestBuilder {
        self.client
            .post(format!("{}/rest/v1/{}", self.url, table))
            .header("apikey",self.anon_key.clone())
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
    }

    pub fn auth_post(&self, path: &str) -> RequestBuilder {
        self.client
            .post(format!("{}/auth/v1/{}", self.url, path))
            .header("apikey",self.anon_key.clone())
            .header("Authorization", format!("Bearer {}", self.anon_key.clone()))
            .header("Content-Type", "application/json")
    }

    pub fn auth_get(&self, path: &str) -> RequestBuilder {
        self.client
            .get(format!("{}/auth/v1/{}", self.url, path))
            .header("apikey",self.anon_key.clone())
            .header("Authorization", format!("Bearer {}", self.token))
    }

    // Supabase's "update user" endpoint (email/password changes) — needs
    // the user's own access token, not the anon key, unlike auth_post
    // (signup/login/refresh, which happen before any token exists).
    pub fn auth_put(&self, path: &str) -> RequestBuilder {
        self.client
            .put(format!("{}/auth/v1/{}", self.url, path))
            .header("apikey",self.anon_key.clone())
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
    }

    pub fn get(&self, table: &str) -> RequestBuilder {
        self.client
            .get(format!("{}/rest/v1/{}", self.url, table))
            .header("apikey",self.anon_key.clone())
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
    }

    pub fn patch(&self, table: &str, id: &str) -> RequestBuilder {
        self.client
            .patch(format!("{}/rest/v1/{}?id=eq.{}", self.url, table, id))
            .header("apikey",self.anon_key.clone())
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
    }

    pub fn delete(&self, table: &str, id: &str) -> RequestBuilder {
        self.client
            .delete(format!("{}/rest/v1/{}?id=eq.{}", self.url, table, id))
            .header("apikey",self.anon_key.clone())
            .header("Authorization", format!("Bearer {}", self.token))
    }

    // Bulk delete — everything RLS scopes to the current token, not a
    // single row by id like `delete` above. PostgREST requires some filter
    // on a DELETE, and `id=not.is.null` matches every row without narrowing
    // further than whatever RLS itself already allows.
    pub fn delete_all(&self, table: &str) -> RequestBuilder {
        self.client
            .delete(format!("{}/rest/v1/{}?id=not.is.null", self.url, table))
            .header("apikey",self.anon_key.clone())
            .header("Authorization", format!("Bearer {}", self.token))
    }

    // Supabase Edge Functions — a different path prefix than the REST/Auth
    // APIs above (see SupabaseStorage::delete_account, the one caller today).
    // The user's own access token in Authorization satisfies both Supabase's
    // platform-level JWT check (on by default for a deployed function) and
    // the function's own internal re-verification of who's calling.
    pub fn functions_post(&self, path: &str) -> RequestBuilder {
        self.client
            .post(format!("{}/functions/v1/{}", self.url, path))
            .header("apikey",self.anon_key.clone())
            .header("Authorization", format!("Bearer {}", self.token))
    }
}
