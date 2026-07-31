use std::env;

fn main() {
    dotenv::from_path(".env").ok();
    
    let supabase_url = env::var("SUPABASE_PUBLIC_URL")
        .unwrap_or_else(|_| "http://localhost:8000".to_string());
    let anon_key = env::var("ANON_KEY")
        .unwrap_or_else(|_| "placeholder".to_string());
    // Cloudflare Turnstile site key (public, safe to ship to the client —
    // see views/auth.rs). Defaults to empty, not a placeholder like the
    // two above: an empty value is the actual "not configured yet" signal
    // the client checks at runtime to skip rendering the widget entirely,
    // rather than a build-breaking missing env!() lookup.
    let turnstile_site_key = env::var("TURNSTILE_SITE_KEY").unwrap_or_default();

    println!("cargo:rustc-env=SUPABASE_PUBLIC_URL={}", supabase_url);
    println!("cargo:rustc-env=SUPABASE_ANON_KEY={}", anon_key);
    println!("cargo:rustc-env=TURNSTILE_SITE_KEY={}", turnstile_site_key);
}
