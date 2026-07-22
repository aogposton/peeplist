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
pub use moment::RecentlyDeletedViewCmp;
pub use moment::UrgencySettingsCmp;

mod sidebar;
pub use sidebar::views_list_cmp;
pub use sidebar::entity_list_cmp;
pub use sidebar::tag_list_cmp;
pub use sidebar::project_list_cmp;

mod entity;
pub use entity::EntityModalCmp;
pub use entity::entity_view_cmp;
pub use entity::ab_history_cmp;
pub use entity::ab_stats_cmp;
pub use entity::ab_info_cmp;
pub use entity::DistanceViewCmp;
pub(crate) use entity::compute_distance;
pub(crate) use entity::backdated_created_at_for_distance;

mod graph;
pub use graph::GraphViewCmp;

mod settings;
pub use settings::SettingsCmp;

pub mod context_menu;
