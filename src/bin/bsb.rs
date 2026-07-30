// Black Server Book CLI ("bsb") — a genuinely standalone binary: it does
// NOT require the desktop GUI app, only the `native` Cargo feature (real
// std::fs + a minimal Tokio runtime, no webview/GUI deps at all). Reuses
// the exact same storage/auth logic as the GUI (peeplist::api) rather than
// re-implementing it, so the Local vault this writes to is byte-for-byte
// what the desktop app reads, and Synced-vault behavior can't drift from
// the GUI's own.
//
// Build/run: `cargo run --features native --bin bsb -- <args>`
//
// Usage:
//   bsb add "<text>" [--synced]   quick-capture syntax same as the GUI
//                                 composer: @name, priority:H, due:tomorrow,
//                                 +tag. Local vault by default; --synced
//                                 targets your logged-in Synced account.
//   bsb list [--synced]           open (uncompleted) moments
//   bsb login                     log in to your Synced (Supabase) account
//   bsb logout                    forget the saved session
//   bsb whoami                    show your Local vault's Self entity, plus
//                                  your Synced account if logged in

use peeplist::api::{self, ActiveStorage, VaultKind};
use peeplist::quick_capture;
use peeplist::types::{MomentMetadata, NewMomentType};
use std::io::{self, Write};

// Credential persistence — deliberately a plain JSON file (chmod 600 on
// Unix), not an OS keychain. No crypto exists anywhere else in this
// project yet (see memory project_app_vision — end-to-end encryption is a
// known, unaddressed gap, not something to quietly half-solve here just
// for the CLI), and OS-keychain integration is a much bigger, platform-
// fragmented addition nobody's asked for. Lives outside the vault root on
// purpose — the vault is meant to be portable/human-inspectable flat
// files, and a credential doesn't belong mixed into that.
mod session {
    use serde::{Deserialize, Serialize};
    use std::fs;
    use std::path::PathBuf;

    #[derive(Serialize, Deserialize, Clone)]
    pub struct Session {
        pub access_token: String,
        pub refresh_token: String,
        pub expires_in: i64,
        pub obtained_at: i64,
        pub email: String,
    }

    fn session_path() -> Result<PathBuf, String> {
        let dirs = directories::ProjectDirs::from("", "", "peeplist")
            .ok_or("couldn't resolve a config directory for this platform")?;
        let dir = dirs.config_dir();
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        Ok(dir.join("session.json"))
    }

    pub fn load() -> Option<Session> {
        let path = session_path().ok()?;
        let raw = fs::read_to_string(path).ok()?;
        serde_json::from_str(&raw).ok()
    }

    pub fn save(sess: &Session) -> Result<(), String> {
        let path = session_path()?;
        let raw = serde_json::to_string_pretty(sess).map_err(|e| e.to_string())?;
        fs::write(&path, raw).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&path).map_err(|e| e.to_string())?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(&path, perms).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn clear() -> Result<(), String> {
        let path = session_path()?;
        if path.exists() {
            fs::remove_file(path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

fn now_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

// Refreshes the access token if it's past expiry, saving the refreshed
// session back to disk. Every command that touches the Synced vault routes
// through this first, so a session surviving days between CLI invocations
// just quietly renews instead of forcing a re-login every time.
async fn ensure_fresh_token(sess: session::Session) -> Result<String, String> {
    if now_unix() < sess.obtained_at + sess.expires_in {
        return Ok(sess.access_token);
    }
    let refreshed = api::refresh_access_token(sess.refresh_token).await
        .map_err(|e| format!("your saved session expired and couldn't be refreshed ({e}) — run `bsb login` again"))?;
    let new_sess = session::Session {
        access_token: refreshed.access_token.clone(),
        refresh_token: refreshed.refresh_token,
        expires_in: refreshed.expires_in,
        obtained_at: now_unix(),
        email: refreshed.user.email,
    };
    session::save(&new_sess)?;
    Ok(new_sess.access_token)
}

fn prompt(label: &str) -> Result<String, String> {
    print!("{label}");
    io::stdout().flush().map_err(|e| e.to_string())?;
    let mut line = String::new();
    io::stdin().read_line(&mut line).map_err(|e| e.to_string())?;
    Ok(line.trim().to_string())
}

async fn cmd_login() -> Result<(), String> {
    let email = prompt("Email: ")?;
    let password = rpassword::prompt_password("Password: ").map_err(|e| e.to_string())?;
    let resp = api::login(email, password).await?;
    let sess = session::Session {
        access_token: resp.access_token,
        refresh_token: resp.refresh_token,
        expires_in: resp.expires_in,
        obtained_at: now_unix(),
        email: resp.user.email,
    };
    println!("Logged in as {}", sess.email);
    session::save(&sess)
}

fn cmd_logout() -> Result<(), String> {
    session::clear()?;
    println!("Logged out.");
    Ok(())
}

async fn cmd_whoami() -> Result<(), String> {
    let local = ActiveStorage::for_vault(VaultKind::Local, None);
    match local.get_entities().await {
        Ok(entities) => {
            let self_id = VaultKind::Local.resolve_self_entity_id(&entities);
            match self_id.and_then(|id| entities.into_iter().find(|e| e.id == id)) {
                Some(me) => println!("Local vault: {} (~/Documents/Peeplist)", me.name),
                None => println!("Local vault: (no Self entity yet — will be created on first use)"),
            }
        }
        Err(e) => println!("Local vault: couldn't read it ({e})"),
    }

    match session::load() {
        None => println!("Not logged in — run `bsb login` to connect a Synced vault."),
        Some(sess) => {
            let email = sess.email.clone();
            let token = ensure_fresh_token(sess).await?;
            println!("Synced account: {email}");
            let synced = ActiveStorage::for_vault(VaultKind::Synced, Some(token));
            match synced.get_entities().await {
                Ok(entities) => {
                    let self_id = VaultKind::Synced.resolve_self_entity_id(&entities);
                    if let Some(me) = self_id.and_then(|id| entities.into_iter().find(|e| e.id == id)) {
                        println!("Synced self entity: {} (drift {:.1})", me.name, me.drift);
                    }
                }
                Err(e) => println!("Synced vault: couldn't reach it ({e})"),
            }
        }
    }
    Ok(())
}

async fn storage_for(synced: bool) -> Result<ActiveStorage, String> {
    if !synced {
        return Ok(ActiveStorage::for_vault(VaultKind::Local, None));
    }
    let sess = session::load()
        .ok_or("not logged in — run `bsb login` first, or drop --synced to use your Local vault")?;
    let token = ensure_fresh_token(sess).await?;
    Ok(ActiveStorage::for_vault(VaultKind::Synced, Some(token)))
}

async fn cmd_add(text: &str, synced: bool) -> Result<(), String> {
    let storage = storage_for(synced).await?;
    let vault_kind = if synced { VaultKind::Synced } else { VaultKind::Local };

    let entities = storage.get_entities().await.map_err(|e| e.to_string())?;
    let parsed = quick_capture::parse(text, &entities);

    if parsed.title.trim().is_empty() {
        return Err("nothing to add — title was empty after parsing".to_string());
    }

    let entity_id = parsed.entity_id.clone()
        .or_else(|| vault_kind.resolve_self_entity_id(&entities))
        .ok_or("couldn't determine who to attribute this to (no Self entity found in this vault)")?;
    let entity_name = entities.iter().find(|e| e.id == entity_id).map(|e| e.name.clone())
        .unwrap_or_else(|| "someone".to_string());

    let created = storage.create_moment(NewMomentType {
        title: parsed.title.clone(),
        description: None,
        gravity: Some(1),
        entity_id,
        moment_type_id: 1,
        deleted_at: None,
    }).await.map_err(|e| e.to_string())?;

    if let Some(due) = parsed.due_at.clone() {
        storage.update_moment_field(created.id.clone(), "due_at", serde_json::json!(due)).await
            .map_err(|e| e.to_string())?;
    }
    if parsed.has_metadata() {
        let metadata = MomentMetadata {
            tags: parsed.tags_add.clone(),
            sort_index: None,
            priority: parsed.priority.clone(),
            project: parsed.project.clone(),
            scheduled_at: parsed.scheduled_at.clone(),
            until_at: parsed.until_at.clone(),
            depends_on: Vec::new(),
            additional_entity_ids: Vec::new(),
        };
        storage.update_moment_field(created.id.clone(), "metadata", serde_json::json!(metadata)).await
            .map_err(|e| e.to_string())?;
    }

    println!("Added \"{}\" for {}{}", parsed.title, entity_name,
        parsed.due_at.as_ref().map(|d| format!(" (due {})", &d[..10.min(d.len())])).unwrap_or_default());
    Ok(())
}

async fn cmd_list(synced: bool) -> Result<(), String> {
    let storage = storage_for(synced).await?;
    let entities = storage.get_entities().await.map_err(|e| e.to_string())?;
    let moments = storage.get_moments().await.map_err(|e| e.to_string())?;

    let entity_name = |id: &str| entities.iter().find(|e| e.id == id).map(|e| e.name.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    let mut open: Vec<_> = moments.iter().filter(|m| m.completed_at.is_none()).collect();
    open.sort_by(|a, b| a.due_at.cmp(&b.due_at));

    if open.is_empty() {
        println!("Nothing open.");
        return Ok(());
    }

    for m in &open {
        let due = m.due_at.as_ref().map(|d| format!("  due {}", &d[..10.min(d.len())])).unwrap_or_default();
        let kind = match m.moment_type_id {
            2 => " [promise]",
            3 => " [note]",
            _ => "",
        };
        println!("- {} — {}{}{}", m.title, entity_name(&m.entity_id), kind, due);
    }
    Ok(())
}

fn print_help() {
    println!("Black Server Book CLI (bsb)\n\nUsage:\n  bsb add \"<text>\" [--synced]   quick-capture: @name, priority:H, due:tomorrow, +tag\n  bsb list [--synced]           open moments\n  bsb login                     log in to your Synced account\n  bsb logout                    forget the saved session\n  bsb whoami                    show your Local vault + Synced account (if logged in)\n\nLocal vault is the default target; pass --synced to act on your logged-in Synced account instead.");
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let synced = args.iter().any(|a| a == "--synced");

    let result = match args.get(1).map(String::as_str) {
        Some("add") => match args.get(2).filter(|a| a.as_str() != "--synced") {
            Some(text) => cmd_add(text, synced).await,
            None => Err("usage: bsb add \"<text>\" [--synced]".to_string()),
        },
        Some("list") => cmd_list(synced).await,
        Some("login") => cmd_login().await,
        Some("logout") => cmd_logout(),
        Some("whoami") => cmd_whoami().await,
        _ => {
            print_help();
            return;
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
