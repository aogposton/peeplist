# Design pass — progress & next steps

Working doc for an ongoing UI redesign pass on peeplist (Dioxus 0.7 Rust app). Read this first before touching styling in this repo again — it has context that isn't obvious from the code alone.

## The goal

The user is not a designer and wants the app to have a solid visual baseline. Approach agreed with the user:

- Use **lumen-blocks** (already a Cargo dependency, git tag `v0.3.0`) as the component library for the *visual* layer.
- **Do not remove or restructure functionality** — only swap the visual elements (markup/classes/component choice), keep signals, handlers, and logic intact.
- **Keep the current color palette** (see `src/theme.rs`: `BG`, `HL` = `#e94f37`, etc.) — no palette redesign.
- Go **one component at a time**, show the user each one, wait for confirmation before moving to the next.
- **Exception carved out by the user, later superseded**: the task-input / entity-selector bar was initially called out as already looking good — leave it alone. That got broken as a side effect of the theme wiring (see old "Known issue", now resolved below). In a later session the user explicitly asked to redesign the whole center section including this input bar, superseding the original "leave it alone" instruction — it's now been intentionally restyled, not just restored.

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
- **Dead-session bug (separate later session, functional not visual)**: the Supabase access token could die server-side (expiry/revocation) with zero client-side signal — `getMoments`/`getEntities` would fail to `.json()`-decode the error body Supabase returns and just log to console, leaving the user looking at a silently empty "logged in" app after a refresh. Fixed:
  - `src/api/client.rs`: added `auth_get`, an `/auth/v1/{path}` request authenticated with the *user's* token (existing `auth_post` always used the anon key, which is right for login but wrong for validating a session).
  - `src/api/auth.rs`: added `get_current_user` (hits `GET /auth/v1/user` to check a token is still accepted) and `refresh_access_token` (calls Supabase's `grant_type=refresh_token` flow).
  - `src/views/auth.rs`: login now also persists `refresh_token` to localStorage (previously only `access_token` was kept, even though `LoginResponse` already carried it); logout clears both.
  - `src/layouts/navbar.rs` (`Navbar`, since it's the layout mounted inside the Router and can call `navigator()`): on every load, validates the cached token via `get_current_user`; if rejected, clears storage + `AppState` and redirects to `Route::LoginCMP`. Also starts a background loop (`TOKEN_REFRESH_INTERVAL_MS` = 50 min, via `gloo_timers`) that proactively calls `refresh_access_token` so a live session ideally never reaches the dead-token state at all; if refresh itself fails (e.g. refresh token revoked), it logs the user out the same way.
  - Verified via Playwright: injecting a fake token into `localStorage` and reloading now correctly logs the "Session check failed..." message, clears storage, and lands on `/login`; a normal unauthenticated load (no token) is unaffected.

## Visual pass — completed components

- **Login screen** (`src/views/auth.rs`, `LoginCMP`): rebuilt with lumen-blocks `Input`, `Label`, `Button` inside a card (`border`, `rounded-lg`, `shadow-sm`). Same form signals, same `submitform`/enter-to-submit/error-message logic as before — visuals only.
- **Left sidebar**: `src/layouts/navbar.rs` (`Sidebar`, `profile_cmp`, and the duplicated desktop panel inline in `Navbar()`) and `src/components/sidebar.rs` (`peep_list_cmp`).
  - Container now uses `bg-background` / `border-border` instead of hardcoded theme colors.
  - Nav links (Login/Profile/Crisis View/Logout, and the entity list "All"/"Self"/entities) restyled into a consistent `NAV_LINK_CLASS` nav-item look (rounded, `hover:bg-muted`). Logout uses a destructive-tinted variant. Note: `NAV_LINK_CLASS` is duplicated as a local `const` in both `navbar.rs` and `sidebar.rs` (not shared/exported) — intentional, kept simple rather than adding cross-module coupling.
  - "Add entity +" button keeps the `HL` brand accent color but is now a real button with CSS `hover:` instead of a JS-driven hover-opacity signal (removed an `isHovering` signal that's no longer needed).
  - Dropped the "- " prefix on entity list item text (cosmetic only).

- **Center-section header** (`src/components/entity.rs`, `entity_view_cmp`): rebuilt with a lumen-blocks `Avatar`/`AvatarFallback` (initial letter) above the entity name (or "All" when nothing selected), wrapped in a padded container with a `border-b` divider. The Info/History/Stats/Graphs row is now lumen-blocks `Button`s (`Ghost` when inactive, `Secondary` when active) instead of unstyled `<a>` tags. The Stats panel is now two bordered/rounded cards in a responsive grid instead of loose `flex justify-around` divs.
  - Small bug fix needed for the new active/inactive button styling to make sense: "Info" and "History" previously both toggled the same `is_stats_open` signal (copy-paste leftover) — each now toggles its own signal. No new panel content was added; Info/Graphs still render nothing when toggled, same as before.
- **Moment input bar** (`src/components/moment.rs`, `MomentInputCmp`): this is the component from the old "Known issue" below — now intentionally redesigned rather than restored. Previously the entity-picker `Dropdown`/`Button` lived in the `Input`'s `icon_right` slot, absolutely positioned inside a manually-widened (`w-[calc(100%-16px)]`) input — once the theme tokens made lumen-blocks styling real, this caused the entity picker to render pinned to the far-right edge of the whole content column, visually disconnected from the input box (confirmed via screenshot). Fixed by moving the `Dropdown` out of `icon_right` into a flex sibling next to the `Input` (`full_width: true` instead of the calc-width hack), and wrapping the whole row + the mobile-only "Add Moment" submit button in a bordered/rounded/shadowed card (`border-border`, `rounded-lg`, `shadow-sm`) consistent with the rest of the app. The submit button now uses the same `HL`-brand-accent style as the sidebar's "+ Add entity" button (brand color reserved for "create" actions) instead of a hardcoded `bg-slate-800`. Same form signals/submit logic/enter-to-submit behavior as before — visuals only, plus the one relocated element and the removal of a few stale full-line comments in the area being rewritten anyway.
  - Verified via `dx serve` + headless-browser screenshots (desktop, mobile-width, and clicking the dropdown open) — no console errors beyond expected 401s from unauthenticated API calls.
- **Moment lists** (`src/components/moment.rs`): the section under the input bar.
  - `CheckboxCmp`: restyled to design tokens (`border-input`, `checked:bg-primary`, `rounded-md`) instead of hardcoded `slate-300`/`blue-600`; dropped a stray `shadow-inner` wrapper class. Same props/logic, still used by both `MomentCmp` and `ab_task_cmp` (left untouched there — out of scope).
  - `NotesSectionCmp`/`CompletedSectionCmp`: replaced the hand-rolled expand/collapse (local `expanded` signal + rotating `⌄` span) with lumen-blocks `Collapsible`/`CollapsibleTrigger`/`CollapsibleContent`, wrapped in a bordered/rounded card matching the rest of the app. Same filtering logic (by `moment_type_id`/`completed_at`), same (structurally vestigial, pre-existing) `show_menu`/`menu_coords` signals — not wired to render anything in these two components, same as before, left alone since fixing that is a functional change, not visual.
  - `MomentListCmp`: active-tasks list is now one bordered/rounded card with `divide-y` row separators instead of loose `px-4` divs. The right-click context menu (`Convert to promise/Note/Task`) restyled to `bg-popover`/`border-border` tokens with proper item hover states; also switched its positioning from `absolute` to `fixed` since it's built from `client_coordinates()` (viewport-relative) — cosmetic/positioning fix only, same convert-to logic.
  - `MomentCmp` (the row itself): promise entries (`moment_type_id == 2`) now get a left accent bar in `HL` plus a small "Promise" label under the title, in addition to the existing pink background tint — makes the promise/task visual distinction explicit rather than just background-color-only. Kept the exact same `bg`/`visual_opacity` signal logic (same palette, `BGpromise`/`BGpromiseHover`/etc. from `theme.rs`, unchanged), just cleaned up a duplicated `transition: opacity` declaration and switched the row to a flex layout with `truncate` on the title.
- **Mobile input bar + FAB** (`src/layouts/navbar.rs`, `src/views/home.rs`): on mobile the task-input bar is now hidden by default and only appears when the "+" button is tapped, so the list gets the screen space back.
  - `home.rs`: both `MomentInputCmp {}` call sites (Inbox and Entity/SELF branches) wrapped in `div { class: "hidden xl:block", ... }` — unchanged/always-visible on desktop (`xl:` and up), hidden below that breakpoint.
  - `navbar.rs`: the `#add-moment-button` FAB is now `xl:hidden` (mobile-only — desktop already shows the input inline, no need for the button there), restyled to a proper circular FAB (`h-14 w-14 rounded-full`, centered `+`/`✕`, `hover:scale-105`/`active:scale-95`), and moved off `z-10` to `z-51` (same tier as the hamburger) so it stays clickable above the backdrop.
  - There was a pre-existing, not-fully-wired second `MomentInputCmp {}` render inside the main flex row (`if *momentInputTgl.read() { MomentInputCmp {} }`) — it rendered as a flex sibling next to the sidebar/content/activity-bar columns rather than as a popup, which would have broken the layout. Replaced it with a `fixed inset-x-0 bottom-24 z-50` wrapper (always mounted on mobile, `xl:hidden`, cross-faded/slid via `opacity`/`translate-y` on `momentInputTgl` — same always-mounted-plus-class-toggle pattern the `Sidebar` already uses) so opening/closing animates instead of hard-mounting.
  - The shared backdrop (`#backdrop`) now also shows for `*momentInputTgl.read()` (previously only `backdropTgl`/`sidebarTgl`), so the mobile popup dims the background and tapping outside it closes it — reuses the backdrop's existing click handler, which already reset `momentInputTgl` to `false`.
  - Verified via headless-browser screenshots at a 390×844 mobile viewport: input hidden by default, "+" opens the popup card above the FAB with the backdrop dimmed and the button flipped to "✕", tapping the backdrop closes it again. No console errors beyond expected 401s.

## Not yet touched (candidates for "what's next")

- `src/components/entity.rs` `EntityModalCmp` (the "New Entity" modal) — still plain HTML `input`/`select`, not restyled.
- `src/layouts/navbar.rs::Navbar()` activity bar (`#activity-bar`) / `ab_task_cmp` (the activity-bar detail panel, still using raw `input`/`textarea`/`button_cmp`) — not touched in this pass.
- `src/components/context_menu/`.
- `src/ui.rs` custom components (`button_cmp`, `gravity_select`) — still used elsewhere (e.g. nowhere critical left after login rework, but check before deleting anything — the user does not want components erased, only restyled).

## State of the working tree

The foundational-setup + login/sidebar work from earlier in this pass got committed by the user at some point as `89af106 "towards a better ui"` — don't assume everything above is still uncommitted; check `git log`/`git status` fresh each session rather than trusting this doc's file list. As of the end of *this* session, uncommitted on top of that commit:
```
M src/api/auth.rs
M src/api/client.rs
M src/api/mod.rs
M src/components/entity.rs
M src/components/moment.rs
M src/layouts/navbar.rs
M src/views/auth.rs
M src/views/home.rs
```
Ask the user before committing/pushing anything; they haven't asked for a commit yet.

A `dx serve` instance may still be running in the background on `http://localhost:8080` from verifying this pass — check for a stray process before starting another one, or just reuse it.
