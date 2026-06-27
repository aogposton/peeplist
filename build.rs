use std::env;

fn main() {
    dotenv::from_path(".env").ok();
    
    let supabase_url = env::var("SUPABASE_PUBLIC_URL")
        .unwrap_or_else(|_| "http://localhost:8000".to_string());
    let anon_key = env::var("ANON_KEY")
        .unwrap_or_else(|_| "placeholder".to_string());
    
    println!("cargo:rustc-env=SUPABASE_PUBLIC_URL={}", supabase_url);
    println!("cargo:rustc-env=SUPABASE_ANON_KEY={}", anon_key);
}
