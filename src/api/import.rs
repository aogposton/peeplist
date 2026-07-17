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
use crate::types::*;
use std::collections::HashMap;

pub struct ImportSummary {
    pub entities: usize,
    pub moments: usize,
    pub reactions: usize,
}

pub async fn import_local_into_synced(token: String) -> Result<ImportSummary, String> {
    let local = ActiveStorage::for_vault(VaultKind::Local, None);
    let synced = ActiveStorage::for_vault(VaultKind::Synced, Some(token));

    let local_entities = local.get_entities().await.map_err(|e| e.to_string())?;
    let local_moments = local.get_moments().await.map_err(|e| e.to_string())?;

    // Self is never copied as a new entity — each vault already has its
    // own self-identity, under a different id convention on each backend
    // (LOCAL_SELF_ENTITY_ID "self" locally vs. SELF_ENTITY_ID "0" for
    // Supabase — the two were never reconciled, see memory
    // project_self_entity_convention). Moments attributed to Self locally
    // get re-attributed to the Synced vault's own Self instead of creating
    // a duplicate "Self" person.
    let mut id_map: HashMap<String, String> = HashMap::new();
    let mut entities_imported = 0usize;

    for entity in local_entities.iter().filter(|e| !is_self_entity(&e.id)) {
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
        let target_entity_id = if is_self_entity(&moment.entity_id) {
            SELF_ENTITY_ID.to_string()
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

    Ok(ImportSummary { entities: entities_imported, moments: moments_imported, reactions: reactions_imported })
}
