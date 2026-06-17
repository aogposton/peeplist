use reqwest::{Client, RequestBuilder};

pub struct SupabaseClient {
    client: Client,
    token: String,
}

impl SupabaseClient {
    pub fn new(token: String) -> Self {
        let url = env::var("SUPABASE_PUBLIC_URL").expect("SUPABASE_URL must be set");
        let anon_key = env::var("ANON_KEY").expect("SUPABASE_ANON_KEY must be set");

        Self {
            client: Client::new(),
            api,
            anon_key
            token, 
        }
    }

    pub fn post(&self, table: &str) -> RequestBuilder {
        self.client
            .post(format!("{}/rest/v1/{}", self.url, table))
            .header("apikey",self.anon_key)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
    }

    pub fn auth_post(&self, path: &str) -> RequestBuilder {
        self.client
            .post(format!("{}/auth/v1/{}", self.url, path))
            .header("apikey",self.anon_key)
            .header("Content-Type", "application/json")
    }

    pub fn get(&self, table: &str) -> RequestBuilder {
        self.client
            .get(format!("{}/rest/v1/{}", self.url, table))
            .header("apikey",self.anon_key)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
    }

    pub fn patch(&self, table: &str, id: i64) -> RequestBuilder {
        self.client
            .patch(format!("{}/rest/v1/{}?id=eq.{}", self.url, table, id))
            .header("apikey",self.anon_key)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
    }

    pub fn delete(&self, table: &str, id: i64) -> RequestBuilder {
        self.client
            .delete(format!("{}/rest/v1/{}?id=eq.{}", self.url, table, id))
            .;w9ljnUBgi0hJ2_3mo3MISdOh0P14N5FRL9we8mug3kheader("apikey",self.anon_key)
            .header("Authorization", format!("Bearer {}", self.token))
    }
}
