# Design pass — progress & next steps

Working doc for an ongoing UI redesign pass on peeplist (Dioxus 0.7 Rust app). Read this first before touching styling in this repo again — it has context that isn't obvious from the code alone.

## The goal

The user is not a designer and wants the app to have a solid visual baseline. Approach agreed with the user:

- Use **lumen-blocks** (already a Cargo dependency, git tag `v0.3.0`) as the component library for the *visual* layer.
- **Do not remove or restructure functionality** — only swap the visual elements (markup/classes/component choice), keep signals, handlers, and logic intact.
- **Keep the current color palette** (see `src/theme.rs`: `BG`, `HL` = `#e94f37`, etc.) — no palette redesign.
- Go **one component at a time**, show the user each one, wait for confirmation before moving to the next.
- **Exception carved out by the user**: the task-input / entity-selector bar on the main screen (in `src/components/moment.rs`, the `Input` with the entity `Dropdown` in `icon_right`) was explicitly called out as already looking good — leave it alone. (See "Known issue" below — this got broken as a side effect and is not yet fixed.)

The user is new to Claude Code and is (reasonably) cautious about big/destructive changes — keep changes scoped, explain before broad or foundational edits, and don't go touch things that weren't asked for.

## Foundational setup (done)

lumen-blocks was already in `Cargo.toml`/`Cargo.lock` and partially used in `src/components/entity.rs` and `src/components/moment.rs`, but it wasn't actually rendering styled — two things were missing, both now fixed:

1. **`assets/dx-components-theme.css` wasn't linked.** Added it as a `document::Link` in `src/main.rs` (alongside `MAIN_CSS`/`TAILWIND_CSS`).
2. **Tailwind didn't know the color tokens lumen-blocks uses** (`bg-background`, `text-foreground`, `border-input`, `text-destructive`, etc. — shadcn-style tokens). Fixed in the root `tailwind.css` (the *source* file Dioxus compiles from — not `assets/tailwind.css`, which is generated output, don't hand-edit it):
   - Added a `:root { --background: ...; --primary: ...; ... }` block + `@theme inline { --color-background: var(--background); ... }` mapping, copied from lumen-blocks' own docsite config.
   - Added `@source "/Users/aogposton/.cargo/git/checkouts/lumen-blocks-5600ca768664ebbd/57eda26/blocks/src/**/*.rs";` so Tailwind's scanner picks up class names used *inside* the lumen-blocks crate itself (it can't generate utility classes it never sees referenced in scanned source).
   - **Caveat**: that `@source` path is machine-specific (points into this user's `~/.cargo/git/checkouts/...`, which includes a resolved-commit hash). It'll keep working as-is on this machine unless `cargo update` re-resolves the lumen-blocks git dependency or someone builds from a different machine — if lumen-blocks components suddenly look unstyled again, check this path first.
   - Deliberately did **not** add the dark-mode `@media (prefers-color-scheme: dark)` override block — the user wants to keep the current (light) palette, not have it flip based on OS theme.
3. To verify/regenerate the compiled `assets/tailwind.css`, run `dx build --platform web` (or `dx serve`). `cargo check` alone does **not** touch the CSS pipeline.

## Bug fixes (done, unrelated to visual pass but done in the same session)

- `src/views/auth.rs`: login previously failed **silently** (error only went to `clog!`/browser console). Now shows a visible error message and redirects to `Route::Home` automatically if the user is already authenticated (`state.auth_token` is `Some`).
- `src/layouts/navbar.rs`: the "login" link in the sidebar (`profile_cmp`) is now hidden once `state.auth_token` is set.

## Visual pass — completed components

- **Login screen** (`src/views/auth.rs`, `LoginCMP`): rebuilt with lumen-blocks `Input`, `Label`, `Button` inside a card (`border`, `rounded-lg`, `shadow-sm`). Same form signals, same `submitform`/enter-to-submit/error-message logic as before — visuals only.
- **Left sidebar**: `src/layouts/navbar.rs` (`Sidebar`, `profile_cmp`, and the duplicated desktop panel inline in `Navbar()`) and `src/components/sidebar.rs` (`peep_list_cmp`).
  - Container now uses `bg-background` / `border-border` instead of hardcoded theme colors.
  - Nav links (Login/Profile/Crisis View/Logout, and the entity list "All"/"Self"/entities) restyled into a consistent `NAV_LINK_CLASS` nav-item look (rounded, `hover:bg-muted`). Logout uses a destructive-tinted variant. Note: `NAV_LINK_CLASS` is duplicated as a local `const` in both `navbar.rs` and `sidebar.rs` (not shared/exported) — intentional, kept simple rather than adding cross-module coupling.
  - "Add entity +" button keeps the `HL` brand accent color but is now a real button with CSS `hover:` instead of a JS-driven hover-opacity signal (removed an `isHovering` signal that's no longer needed).
  - Dropped the "- " prefix on entity list item text (cosmetic only).

## Known issue — not yet fixed

The task-input/entity-selector bar (`src/components/moment.rs`, inside `MomentInputCmp` — the `Input` with the `Dropdown`/`Button` entity picker in `icon_right`) was **already using lumen-blocks** before this pass started, but was rendering unstyled because the theme tokens didn't exist yet (see "Foundational setup"). The user liked how it looked *unstyled* and explicitly asked not to touch it — but once the theme tokens were wired up globally, this component picked up lumen-blocks' real default styling too, changing its look as an unintended side effect.

User's call: **leave it for now, fix later** (deferred, not urgent). When picked back up: pin explicit classes on that specific `Input`/`Button`/`Dropdown` usage in `moment.rs` so it opts out of the new defaults and gets back its old look, without reverting the global theme fix (which every other component now depends on).

## Not yet touched (candidates for "what's next")

- Top navbar/header area in `src/layouts/navbar.rs::Navbar()` — hamburger button, floating add-moment button (`#add-moment-button`), activity bar (`#activity-bar`).
- `src/components/entity.rs` (entity modal/collapsible) — already partially uses lumen-blocks, not reviewed/polished yet.
- `src/components/moment.rs` — moment cards/list (`MomentCmp`, `MomentListCmp`), `NotesSectionCmp`, `ab_task_cmp`.
- `src/components/context_menu/`.
- `src/ui.rs` custom components (`button_cmp`, `gravity_select`) — still used elsewhere (e.g. nowhere critical left after login rework, but check before deleting anything — the user does not want components erased, only restyled).

## State of the working tree

As of the end of this session, nothing has been committed — all of the above is uncommitted local changes:
```
M assets/tailwind.css
M src/components/sidebar.rs
M src/layouts/navbar.rs
M src/main.rs
M src/views/auth.rs
M tailwind.css
```
Ask the user before committing/pushing anything; they haven't asked for a commit yet.
