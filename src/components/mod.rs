//! The components module contains all shared components for our app. Components are the building blocks of dioxus apps.
//! They can be used to defined common UI elements like buttons, forms, and modals. In this template, we define a Hero
//! component and an Echo component for fullstack apps to be used in our app.

mod moment;
pub use moment::MomentCmp;
pub use moment::MomentListCmp;
pub use moment::MomentInputCmp;
pub use moment::CompletedSectionCmp;
pub use moment::NotesSectionCmp;
pub use moment::ab_task_cmp;
pub use moment::PriorityViewCmp;

mod sidebar;
pub use sidebar::peep_list_cmp;
pub use sidebar::tag_list_cmp;

mod entity;
pub use entity::EntityModalCmp;
pub use entity::entity_view_cmp;
pub use entity::ab_history_cmp;
pub use entity::ab_stats_cmp;
pub use entity::ab_info_cmp;

pub mod context_menu;
