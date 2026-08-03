// Exists only so `src/bin/*.rs` binaries (the CLI — see bsb.rs) can reuse
// the app's storage/auth/parsing logic without duplicating it. main.rs (the
// GUI, built via `dx build`/`dx serve`) does NOT depend on this — it still
// declares its own `mod` tree exactly as before, so this file is purely
// additive and changes nothing about the existing web/desktop/mobile
// builds. The same source files just get compiled a second time, once per
// target that needs them, which is the trade Cargo expects for a lib+bin(s)
// package without restructuring main.rs itself.
//
// `api` (auth.rs, storage.rs, client.rs, entity.rs, moment.rs,
// local_desktop.rs) turned out to be safely reusable here despite an
// earlier version of this comment claiming otherwise — none of those files
// actually touch AppState/Dioxus signals or web_sys; they're plain
// reqwest+serde+std::fs. Only `api/local.rs` (the wasm/localStorage Local
// vault backend) is GUI/web-only, and it's already excluded from this lib
// build via its own `#[cfg(not(feature = "native"))]` gate in api/mod.rs.

#[path = "types.rs"]
pub mod types;

#[path = "quick_capture.rs"]
pub mod quick_capture;

#[path = "taskwarrior_date.rs"]
pub mod taskwarrior_date;

// Deliberately not also re-declared at this top level — `api` (below)
// already declares `pub mod vault_format;` itself (api/local_desktop.rs
// imports it as `crate::api::vault_format`), and Rust treats two separate
// `#[path]` declarations pointing at the same file as two distinct,
// non-interchangeable types. Reach it via `black_server_book::api::vault_format`.
#[path = "api/mod.rs"]
pub mod api;
