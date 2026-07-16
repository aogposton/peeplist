// Exists only so `src/bin/*.rs` binaries (the CLI — see pltask.rs) can
// reuse the vault file format and quick-capture parser without duplicating
// them. main.rs (the GUI, built via `dx build`/`dx serve`) does NOT depend
// on this — it still declares its own `mod` tree exactly as before, so
// this file is purely additive and changes nothing about the existing
// web/desktop/mobile builds. The same three source files just get compiled
// a second time, once per target that needs them, which is the trade Cargo
// expects for a lib+bin(s) package without restructuring main.rs itself.
//
// Deliberately minimal: only the pure, no-I/O modules the CLI actually
// needs. Not `api` as a whole — that module tree is wired around
// AppState/Dioxus signals and isn't meaningfully reusable outside the GUI.

#[path = "types.rs"]
pub mod types;

#[path = "quick_capture.rs"]
pub mod quick_capture;

#[path = "api/vault_format.rs"]
pub mod vault_format;
