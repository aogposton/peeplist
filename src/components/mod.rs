//! Shared UI components for the app.

mod moment;
pub use moment::MomentCmp;
pub use moment::MomentListCmp;
pub use moment::MomentInputCmp;
pub use moment::CompletedSectionCmp;
pub use moment::NotesSectionCmp;
pub use moment::ab_task_cmp;
pub use moment::PriorityViewCmp;
pub use moment::DueViewCmp;
pub use moment::ScheduledViewCmp;
pub use moment::BlockingViewCmp;
pub use moment::NotesViewCmp;
pub use moment::FullScreenEditorModalCmp;
pub use moment::OnTheFlyCmp;
pub use moment::RecentlyDeletedViewCmp;
pub use moment::UrgencySettingsCmp;
pub use moment::MomentosViewCmp;
pub use moment::MissedViewCmp;
pub use moment::CheckboxCmp;
// Reused by entity.rs's ab_momentos_cmp for completing/skipping a single
// momento occurrence — same read-modify-write-the-whole-metadata-blob
// pattern moment.rs's own field edits already use, not worth a second copy.
pub(crate) use moment::patch_moment_metadata;

mod sidebar;
pub use sidebar::views_list_cmp;
pub use sidebar::entity_list_cmp;
pub use sidebar::tag_list_cmp;
pub use sidebar::project_list_cmp;

mod entity;
pub use entity::entity_view_cmp;
pub use entity::ab_story_cmp;
pub use entity::ab_momentos_cmp;
pub use entity::ab_stats_cmp;
pub use entity::ab_info_cmp;
pub use entity::DistanceViewCmp;
pub use entity::AllEntitiesViewCmp;
pub(crate) use entity::compute_distance;
pub(crate) use entity::backdated_created_at_for_distance;

mod graph;
pub use graph::GraphViewCmp;

mod settings;
pub use settings::SettingsCmp;

pub mod context_menu;
