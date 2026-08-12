// One-time, explicit copy of the Local vault into a Synced vault. Vaults
// are deliberately never auto-merged (see the local-first pivot plan) —
// logging in gives you a second, empty vault, not your existing history
// made portable. This closes that gap: an explicit action, not something
// that runs automatically on login.
//
// Best-effort, not transactional — there's no way to roll back a partial
// failure across a series of individual REST calls to a different backend
// than the one being read from. A failure partway through leaves whatever
// was already created in place; the summary returned only counts
// successes, so a partial import is visible to the caller rather than
// silently reported as complete.

use crate::api::{ActiveStorage, VaultKind, is_self_entity};
use crate::api::vault_format::{self, BackupBundle, LOCAL_SELF_ENTITY_ID};
use crate::types::*;
use std::collections::HashMap;

pub struct ImportSummary {
    pub entities: usize,
    pub moments: usize,
    pub reactions: usize,
    // Synced-only: entities whose exported type name didn't match anything
    // in this vault's entity_types table, imported with no type set rather
    // than failing outright. Always 0 for a Local target (entity_type is
    // free text there, so nothing is ever unmatched) and for
    // import_local_into_synced (a different, older code path — see below).
    pub entities_untyped: usize,
}

pub async fn import_local_into_synced(token: String) -> Result<ImportSummary, String> {
    let local = ActiveStorage::for_vault(VaultKind::Local, None);
    let synced = ActiveStorage::for_vault(VaultKind::Synced, Some(token.clone()));

    let local_entities = local.get_entities().await.map_err(|e| e.to_string())?;
    let local_moments = local.get_moments().await.map_err(|e| e.to_string())?;
    let synced_entities = synced.get_entities().await.map_err(|e| e.to_string())?;
    let synced_self_id = VaultKind::Synced.resolve_self_entity_id(&synced_entities)
        .ok_or_else(|| "Couldn't find the Synced vault's Self entity".to_string())?;

    // Self is never copied as a new entity — each vault already has its
    // own self-identity, under a different id convention on each backend
    // (LOCAL_SELF_ENTITY_ID "self" locally vs. a per-account entity_type
    // match for Synced — the two were never reconciled, see memory
    // project_self_entity_convention). Moments attributed to Self locally
    // get re-attributed to the Synced vault's own Self instead of creating
    // a duplicate "Self" person.
    let mut id_map: HashMap<String, String> = HashMap::new();
    let mut entities_imported = 0usize;

    for entity in local_entities.iter().filter(|e| !is_self_entity(e)) {
        let new_entity = NewEntityType {
            name: entity.name.clone(),
            entity_type_id: entity.entity_type_id.clone(),
            parent_entity_id: None,
            user_id: None,
            archived_at: None,
            metadata: entity.metadata.clone(),
        };
        match synced.create_entity(new_entity).await {
            Ok(created) => {
                id_map.insert(entity.id.clone(), created.id.clone());
                entities_imported += 1;
            }
            // Best-effort: one bad entity shouldn't abort the whole
            // import. Its moments below will be skipped too, since
            // there's no synced id to attribute them to.
            Err(_) => continue,
        }
    }

    let mut moments_imported = 0usize;
    let mut reactions_imported = 0usize;

    for moment in local_moments.iter() {
        let target_entity_id = if moment.entity_id == LOCAL_SELF_ENTITY_ID {
            synced_self_id.clone()
        } else {
            match id_map.get(&moment.entity_id) {
                Some(id) => id.clone(),
                None => continue,
            }
        };

        let new_moment = NewMomentType {
            title: moment.title.clone(),
            description: moment.description.clone(),
            gravity: moment.gravity,
            entity_id: target_entity_id,
            moment_type_id: moment.moment_type_id,
            deleted_at: None,
        };

        let created = match synced.create_moment(new_moment).await {
            Ok(m) => m,
            Err(_) => continue,
        };
        moments_imported += 1;

        // due_at/completed_at/metadata aren't part of NewMomentType — same
        // create-then-patch two-step the GUI composer itself uses (see
        // submit_moment in components/moment.rs). Best-effort: a failed
        // patch here still leaves the moment itself imported.
        if let Some(due) = &moment.due_at {
            let _ = synced.update_moment_field(created.id.clone(), "due_at", serde_json::json!(due)).await;
        }
        if let Some(completed) = &moment.completed_at {
            let _ = synced.update_moment_field(created.id.clone(), "completed_at", serde_json::json!(completed)).await;
        }
        if let Some(meta) = &moment.metadata {
            let _ = synced.update_moment_field(created.id.clone(), "metadata", serde_json::json!(meta)).await;
        }

        for reaction in moment.reactions.iter().flatten() {
            let new_reaction = NewReactionType {
                moment_id: created.id.clone(),
                description: reaction.description.clone(),
                value: reaction.value,
            };
            if synced.create_reaction(new_reaction).await.is_ok() {
                reactions_imported += 1;
            }
        }
    }

    Ok(ImportSummary { entities: entities_imported, moments: moments_imported, reactions: reactions_imported, entities_untyped: 0 })
}

// Full-vault backup/restore (2026-08-01) — see memory: a real data-loss
// incident, and a follow-up security concern about the format leaking
// database internals, are what prompted this. Unlike
// import_local_into_synced above (a one-time, one-direction migration),
// these two work on whatever vault is currently active, in either
// direction — a disaster-recovery pair, not a vault-migration tool. See
// vault_format.rs's BackupBundle doc comment for the format itself (YAML,
// file-relative ref numbers instead of database ids, literal type names).

pub async fn export_backup(vault: VaultKind, token: Option<String>) -> Result<String, String> {
    let storage = ActiveStorage::for_vault(vault, token);
    let entities = storage.get_entities().await.map_err(|e| e.to_string())?;
    let moments = storage.get_moments().await.map_err(|e| e.to_string())?;
    // Local's get_entity_types() already returns an identity map (id ==
    // name — see api/local.rs's DEFAULT_ENTITY_TYPES), so this same lookup
    // resolves entity_type_id -> a literal name uniformly for both
    // backends, no VaultKind branch needed here. Import (below) isn't
    // symmetric — that's where the backend split actually matters.
    let type_names: HashMap<String, String> = storage.get_entity_types().await
        .map_err(|e| e.to_string())?
        .into_iter().map(|t| (t.id, t.name)).collect();
    let bundle = vault_format::build_backup_bundle(&entities, &moments, &type_names, chrono::Utc::now().to_rfc3339());
    vault_format::render_backup(&bundle).map_err(|e| e.to_string())
}

// Best-effort, not transactional, same posture as import_local_into_synced
// above — a partial failure leaves whatever was already created in place,
// and the returned summary only counts successes.
//
// Two passes over entities/moments because ids are only known once a
// record is actually created (Supabase/local both mint fresh ids on
// insert): the first pass creates every entity and every moment, building
// ref -> new-id maps as it goes; the second pass goes back and patches
// each moment's metadata with those maps applied to depends_on and
// additional_entity_refs, since a moment can depend on one that's created
// later in iteration order and so isn't resolvable on the first pass.
pub async fn import_backup(target_vault: VaultKind, token: Option<String>, yaml: String) -> Result<ImportSummary, String> {
    let bundle: BackupBundle = vault_format::parse_backup(&yaml)
        .map_err(|e| format!("That doesn't look like a valid backup file: {e}"))?;

    let target = ActiveStorage::for_vault(target_vault, token);
    let target_entities = target.get_entities().await.map_err(|e| e.to_string())?;
    let target_self_id = target_vault.resolve_self_entity_id(&target_entities)
        .ok_or_else(|| "Couldn't find this vault's Self entity".to_string())?;

    // Entity-type resolution — the one place the backend split actually
    // matters (export needed no branch, see export_backup above). Local:
    // entity_type is free text, always accepted as-is, no lookup. Synced:
    // entity_types is a real, closed, RLS-locked-to-read-only table — match
    // the exported name case-insensitively against it, or leave the type
    // unset if nothing matches, rather than failing the whole import over
    // one unrecognized type name.
    let synced_type_ids: Option<HashMap<String, String>> = match target_vault {
        VaultKind::Local => None,
        VaultKind::Synced => Some(
            target.get_entity_types().await.map_err(|e| e.to_string())?
                .into_iter().map(|t| (t.name.to_lowercase(), t.id)).collect()
        ),
    };
    let resolve_entity_type = |name: &Option<String>| -> Option<String> {
        let name = name.as_ref()?;
        match &synced_type_ids {
            None => Some(name.clone()),
            Some(map) => map.get(&name.to_lowercase()).cloned(),
        }
    };

    // Self is never imported as a new entity — the target vault already has
    // its own. Anything marked `self: true` in the file gets re-attributed
    // to the target vault's Self instead of creating a duplicate person.
    let mut entity_ref_map: HashMap<u32, String> = HashMap::new();
    let mut entities_imported = 0usize;
    let mut entities_untyped = 0usize;

    for doc in bundle.entities.iter() {
        if doc.self_entity {
            entity_ref_map.insert(doc.r#ref, target_self_id.clone());
            continue;
        }
        let resolved_type = resolve_entity_type(&doc.entity_type);
        if doc.entity_type.is_some() && resolved_type.is_none() {
            entities_untyped += 1;
        }
        let new_entity = NewEntityType {
            name: doc.name.clone(),
            entity_type_id: resolved_type,
            // Not remapped here — see the parent_ref patch pass below, once
            // every entity's new id is actually known.
            parent_entity_id: None,
            user_id: None,
            archived_at: None,
            metadata: Some(EntityMetadata::default()),
        };
        match target.create_entity(new_entity).await {
            Ok(created) => {
                entity_ref_map.insert(doc.r#ref, created.id.clone());
                entities_imported += 1;
            }
            // Best-effort: one bad entity shouldn't abort the whole
            // import. Its moments below will be skipped too, since
            // there's no target id to attribute them to.
            Err(_) => continue,
        }
    }

    // Individuation lineage (parent_ref) — patched after the fact now that
    // every entity's new id is known, rather than relied on during
    // creation above.
    for doc in bundle.entities.iter() {
        let (Some(new_id), Some(parent_ref)) = (entity_ref_map.get(&doc.r#ref), doc.parent_ref) else { continue };
        if let Some(new_parent) = entity_ref_map.get(&parent_ref) {
            let _ = target.update_entity_field(new_id.clone(), "parent_entity_id", serde_json::json!(new_parent)).await;
        }
    }

    let mut moment_ref_map: HashMap<u32, String> = HashMap::new();
    // Held back rather than patched immediately — depends_on/
    // additional_entity_refs still hold the *file's* ref numbers and can't
    // be remapped until every entity/moment in this import has a new id.
    let mut pending_metadata: Vec<(String, MomentMetadata, Vec<u32>, Vec<u32>)> = Vec::new();
    let mut moments_imported = 0usize;
    let mut reactions_imported = 0usize;

    for doc in bundle.entities.iter() {
        let Some(target_entity_id) = entity_ref_map.get(&doc.r#ref).cloned() else { continue };
        for entry in doc.moments.iter() {
            let new_moment = NewMomentType {
                title: entry.title.clone(),
                description: entry.description.clone(),
                gravity: entry.gravity,
                entity_id: target_entity_id.clone(),
                moment_type_id: vault_format::moment_type_id(&entry.kind),
                deleted_at: None,
            };
            let created = match target.create_moment(new_moment).await {
                Ok(m) => m,
                Err(_) => continue,
            };
            moment_ref_map.insert(entry.r#ref, created.id.clone());
            moments_imported += 1;

            // due_at/completed_at/metadata aren't part of NewMomentType —
            // same create-then-patch two-step the GUI composer itself uses.
            if let Some(due) = &entry.due_at {
                let _ = target.update_moment_field(created.id.clone(), "due_at", serde_json::json!(due)).await;
            }
            if let Some(completed) = &entry.completed_at {
                let _ = target.update_moment_field(created.id.clone(), "completed_at", serde_json::json!(completed)).await;
            }

            let base_meta = MomentMetadata {
                tags: entry.tags.clone(),
                sort_index: entry.sort_index,
                priority: entry.priority.clone(),
                project: entry.project.clone(),
                scheduled_at: entry.scheduled_at.clone(),
                until_at: entry.until_at.clone(),
                depends_on: vec![],
                additional_entity_ids: vec![],
                // Plain calendar dates, not cross-references — no ref
                // remapping needed, copy straight through.
                recurrence_rule: entry.recurrence_rule.clone(),
                momento_completed_occurrences: entry.momento_completed_occurrences.clone(),
                momento_excluded_occurrences: entry.momento_excluded_occurrences.clone(),
                reveal_lead: entry.reveal_lead.clone(),
            };
            pending_metadata.push((created.id.clone(), base_meta, entry.depends_on.clone(), entry.additional_entity_refs.clone()));

            for reaction in entry.reactions.iter() {
                let new_reaction = NewReactionType {
                    moment_id: created.id.clone(),
                    description: reaction.description.clone(),
                    value: reaction.value,
                };
                if target.create_reaction(new_reaction).await.is_ok() {
                    reactions_imported += 1;
                }
            }
        }
    }

    // Now that every entity and moment in this import has a real target id,
    // remap depends_on/additional_entity_refs and write the final metadata.
    // Anything that doesn't resolve (e.g. pointed at something that failed
    // to import above) is silently dropped rather than left pointing at a
    // stale file-only ref.
    for (new_moment_id, mut meta, depends_on_refs, additional_entity_refs) in pending_metadata {
        meta.depends_on = depends_on_refs.iter().filter_map(|r| moment_ref_map.get(r).cloned()).collect();
        meta.additional_entity_ids = additional_entity_refs.iter().filter_map(|r| entity_ref_map.get(r).cloned()).collect();
        if meta != MomentMetadata::default() {
            let _ = target.update_moment_field(new_moment_id, "metadata", serde_json::json!(meta)).await;
        }
    }

    Ok(ImportSummary { entities: entities_imported, moments: moments_imported, reactions: reactions_imported, entities_untyped })
}
