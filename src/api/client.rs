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
}
