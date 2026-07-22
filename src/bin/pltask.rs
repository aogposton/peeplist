// peeplist CLI, v1 — reads and writes the exact same local vault the
// desktop GUI app uses (~/Documents/Peeplist/), via peeplist::vault_format
// (see src/lib.rs for why that's a separate lib target). Deliberately
// minimal: quick-capture + list, nothing else yet. No network calls, no
// Synced-vault support — CLI only ever touches the Local vault.
//
// Build/run with the `desktop` feature (for the `directories` dependency,
// already optional-gated on it): `cargo run --features desktop --bin pltask -- <args>`
//
// Usage:
//   pltask add "<text>"   quick-capture syntax same as the GUI composer:
//                         @name, priority:H, due:tomorrow, +tag, etc.
//   pltask list           open (uncompleted) moments across the vault

use peeplist::quick_capture;
use peeplist::types::{EntityType, MomentMetadata, MomentType};
use peeplist::vault_format::{self, ParsedEntityFile, LOCAL_SELF_ENTITY_ID, SELF_FILENAME};
use std::fs;
use std::path::{Path, PathBuf};

fn vault_root() -> Result<PathBuf, String> {
    let dirs = directories::UserDirs::new().ok_or("couldn't resolve your home directory")?;
    let docs = dirs.document_dir().ok_or("couldn't resolve your Documents folder")?;
    Ok(docs.join("Peeplist"))
}

fn people_dir() -> Result<PathBuf, String> {
    let dir = vault_root()?.join("people");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn self_path() -> Result<PathBuf, String> {
    Ok(vault_root()?.join(SELF_FILENAME))
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn load_path(path: &Path) -> Option<ParsedEntityFile> {
    let raw = fs::read_to_string(path).ok()?;
    vault_format::parse_entity_file(&raw).ok()
}

fn all_paths() -> Result<Vec<PathBuf>, String> {
    let mut paths: Vec<PathBuf> = fs::read_dir(people_dir()?)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|ext| ext == "md").unwrap_or(false))
        .collect();
    let self_p = self_path()?;
    if self_p.exists() {
        paths.push(self_p);
    }
    Ok(paths)
}

fn all_files() -> Result<Vec<ParsedEntityFile>, String> {
    Ok(all_paths()?.iter().filter_map(|p| load_path(p)).collect())
}

// Same idempotent-open pattern as the GUI backends — first-ever CLI run
// against a fresh vault (or a vault that's only ever been opened via the
// web build, which stores Self under localStorage instead of a file)
// creates self.md rather than erroring.
fn ensure_self_entity() -> Result<(), String> {
    let p = self_path()?;
    if p.exists() {
        return Ok(());
    }
    let self_entity = EntityType {
        id: LOCAL_SELF_ENTITY_ID.to_string(),
        name: "Self".to_string(),
        entity_type_id: None,
        parent_entity_id: None,
        created_at: now(),
        drift: 2.0,
        metadata: None,
    };
    save(&p, &self_entity, &[], "")
}

fn save(path: &Path, entity: &EntityType, moments: &[MomentType], body: &str) -> Result<(), String> {
    let rendered = vault_format::render_entity_file(entity, moments, body);
    let tmp = path.with_extension("md.tmp");
    fs::write(&tmp, rendered).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn find_by_id(id: &str) -> Result<Option<(PathBuf, ParsedEntityFile)>, String> {
    if id == LOCAL_SELF_ENTITY_ID {
        let p = self_path()?;
        return Ok(load_path(&p).map(|f| (p, f)));
    }
    for path in fs::read_dir(people_dir()?).map_err(|e| e.to_string())?.filter_map(|e| e.ok()).map(|e| e.path()) {
        if let Some(file) = load_path(&path) {
            if file.entity.id == id {
                return Ok(Some((path, file)));
            }
        }
    }
    Ok(None)
}

fn cmd_add(text: &str) -> Result<(), String> {
    ensure_self_entity()?;
    let entities: Vec<EntityType> = all_files()?.into_iter().map(|f| f.entity).collect();
    let parsed = quick_capture::parse(text, &entities);

    if parsed.title.trim().is_empty() {
        return Err("nothing to add — title was empty after parsing".to_string());
    }

    let entity_id = parsed.entity_id.clone().unwrap_or_else(|| LOCAL_SELF_ENTITY_ID.to_string());
    let (path, mut file) = find_by_id(&entity_id)?
        .ok_or_else(|| format!("no such person in this vault: {entity_id} (try `pltask list` to see who's tracked)"))?;

    let metadata = if parsed.has_metadata() {
        Some(MomentMetadata {
            tags: parsed.tags_add.clone(),
            sort_index: None,
            priority: parsed.priority.clone(),
            project: parsed.project.clone(),
            scheduled_at: parsed.scheduled_at.clone(),
            until_at: parsed.until_at.clone(),
        })
    } else {
        None
    };

    let moment = MomentType {
        id: uuid::Uuid::new_v4().to_string(),
        title: parsed.title.clone(),
        description: None,
        gravity: Some(1),
        entity_id: file.entity.id.clone(),
        moment_type_id: 1,
        due_at: parsed.due_at.clone(),
        completed_at: None,
        deleted_at: None,
        reactions: None,
        created_at: now(),
        depends_on: None,
        metadata,
    };

    println!("Added \"{}\" for {}{}", moment.title, file.entity.name,
        moment.due_at.as_ref().map(|d| format!(" (due {})", &d[..10.min(d.len())])).unwrap_or_default());

    file.moments.push(moment);
    save(&path, &file.entity, &file.moments, &file.body)
}

fn cmd_list() -> Result<(), String> {
    ensure_self_entity()?;
    let files = all_files()?;
    let mut open: Vec<(String, MomentType)> = files.iter()
        .flat_map(|f| f.moments.iter().map(move |m| (f.entity.name.clone(), m.clone())))
        .filter(|(_, m)| m.completed_at.is_none())
        .collect();
    open.sort_by(|a, b| a.1.due_at.cmp(&b.1.due_at));

    if open.is_empty() {
        println!("Nothing open.");
        return Ok(());
    }

    for (entity_name, m) in &open {
        let due = m.due_at.as_ref().map(|d| format!("  due {}", &d[..10.min(d.len())])).unwrap_or_default();
        let kind = match m.moment_type_id {
            2 => " [promise]",
            3 => " [note]",
            _ => "",
        };
        println!("- {} — {}{}{}", m.title, entity_name, kind, due);
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let result = match args.get(1).map(String::as_str) {
        Some("add") => match args.get(2) {
            Some(text) => cmd_add(text),
            None => Err("usage: pltask add \"<text>\"".to_string()),
        },
        Some("list") => cmd_list(),
        _ => {
            println!("peeplist CLI\n\nUsage:\n  pltask add \"<text>\"   quick-capture: @name, priority:H, due:tomorrow, +tag\n  pltask list            open moments across your local vault");
            return;
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
