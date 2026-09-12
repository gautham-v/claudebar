# claudebar — spec

A macOS menu bar app that shows your Claude usage limits. Click the item, a popover drops down
with three rings (session, week, model) and what you did today, read from the local Claude Code
session logs. Built with GPUI (Rust). Sibling of daybar and mailbar: same stack, same visual
language, same repo layout. MIT, open source.

## Reference
- `docs/mockup-popover.dc.html` — the approved popover (HTML/CSS). Match its tokens, which are
  mailbar's: background `#f0f0f2` light (`#28282a` dark), text `#1d1d1f`, secondary `#6e6e73`,
  tertiary `#aeaeb2`, separators `rgba(0,0,0,0.08)`, accent system blue `#0a7aff`, ring track
  `rgba(0,0,0,0.08)`. Popover **320px** wide, 11px radius, hairline border. Font: system
  (`.SystemUIFont`); numbers in `SF Mono`. All tokens live in `src/ui/theme.rs`.
- `docs/mockup-menubar.dc.html` — the approved menu bar item: percentage then ring, the way the
  battery item sits, and the red state.

## Settings
`src/settings.rs`, a TOML file at `~/.config/claudebar/config.toml`
(`dirs::config_dir()/claudebar/config.toml`), written by the "···" menu and editable by hand:

| key | default | what it does |
|---|---|---|
| `menu_bar` | `"session"` | which limit the menu bar item tracks: `"session"`, `"weekly"`, or a model display name as the API spells it (`"Fable"`) |
| `show_percent` | `true` | `false` draws the ring alone, with no number and no fade |
| `low_remaining_percent` | `20.0` | how little of a window is left before it goes red |
| `refresh_minutes` | `5` | poll interval, read once at startup |

Every key has a serde default, so a partial file works and unknown keys are ignored.
`Settings::load() -> (Settings, Option<String>)`: defaults when the file is missing, and a
human-readable note when it is malformed, which the popover shows as its muted notice line
(never in place of a fetch error). `low_remaining_percent` is clamped to 0..=100 and
`refresh_minutes` to >= 1 on load. `Limit::is_low(low_remaining_percent)` takes the threshold;
`model::LOW_REMAINING_PERCENT` is only the default `Settings` starts from. The provider seam
carries `settings()` / `set_settings()`; `StoreProvider` persists on set and then runs the
on-change hook so the menu bar item is re-titled.

## Data
Two sources, both already on the machine. No account of its own; one optional config file.

1. **Limits** — `GET https://api.anthropic.com/api/oauth/usage` with
   `Authorization: Bearer <accessToken>` and `anthropic-beta: oauth-2025-04-20`. The token is the
   Claude Code OAuth token in the login Keychain: service `Claude Code-credentials`, account =
   the macOS username; the secret is JSON `{"claudeAiOauth": {"accessToken", "expiresAt" (ms),
   "subscriptionType" ("max"/"pro"), ...}}`. Read it with the `keyring` crate (already a dep).
   Response shape (fields we use):
   ```json
   {"five_hour": {"utilization": 2.0, "resets_at": "2026-09-12T22:29:59.705239+00:00"},
    "seven_day": {"utilization": 18.0, "resets_at": "2026-09-16T23:59:59.705258+00:00"},
    "limits": [
      {"kind": "session", "percent": 2, "resets_at": "...", "scope": null},
      {"kind": "weekly_all", "percent": 18, "resets_at": "...", "scope": null},
      {"kind": "weekly_scoped", "percent": 22, "resets_at": "...",
       "scope": {"model": {"id": null, "display_name": "Fable"}}}]}
   ```
   Map to `model::Usage`: session from `five_hour`, weekly from `seven_day`, then one
   `LimitKind::Model(display_name)` per `weekly_scoped` entry in `limits` (in order). Unknown
   fields are ignored; missing `limits` is fine. `plan` = `subscriptionType` capitalised.
   Errors, phrased for the popover's muted line: no Keychain item → `ProviderState::SignedOut`
   ("Sign in with `claude` first"); token expired per `expiresAt` or a 401 → error
   "Token expired — run `claude` to refresh"; network → "Offline". Never refresh the token
   ourselves; Claude Code owns it. Poll every 5 minutes and whenever the popover opens.
2. **Today / last 7 days** — walk `~/.claude/projects/*/*.jsonl`. Each line is JSON; the lines
   that matter have `"type":"assistant"` with `timestamp` (RFC 3339, UTC), `sessionId`,
   `requestId`, and `message.{id, model, usage{input_tokens, output_tokens,
   cache_read_input_tokens, cache_creation_input_tokens}, content[]}`. The same message is
   appended several times as it streams: **dedupe on `(requestId, message.id)`**, keeping the
   last occurrence. Tokens = the four usage fields summed. A tool call is a `content` block with
   `"type":"tool_use"`. A session counts on the day it has ≥1 assistant message. Days are local
   dates. Only read files modified in the last 8 days (mtime), and only the last 7 local days.
   Read on a background thread; the scan is a few hundred MB of JSON at worst, keep it linear
   and streaming (`BufRead::lines`, `serde_json::from_str` per line, skip lines that fail).

The shared types are in `src/model.rs`; the UI seam is `src/ui/provider.rs`
(`UsageProvider`, `StubProvider`). Do not change their public shape without updating both.

## Menu bar item
- `NSStatusItem` with the tracked limit's percentage as the title **first** and the ring image
  **after** it (`NSCellImagePosition::ImageTrailing`, `imageHugsTitle`), menu bar font, so it
  sits like the battery item. Title "2%" (whole number). While loading or signed out: no title,
  ring at 0%, `appearsDisabled`.
- The ring: 16×16pt, drawn at runtime with Core Graphics (`src/menu_bar_icon.rs`): a track
  circle at 30% alpha, 2.5pt stroke, and an arc from 12 o'clock clockwise for the session's
  percent, round caps. Normally a template image so the menu bar tints it. **With `low_remaining_percent`
  or less of the window left (`Limit::is_low`), both the number and the ring go red**: the ring is
  drawn as a non-template image in `NSColor::systemRedColor` and the title gets an attributed
  string with the same colour. No amber, no other colours. Redraw whenever a fetch lands.
- Click toggles the popover; clicking outside or pressing Esc closes it. No Dock icon.

## Popover (320 wide, height fits content)
Top to bottom, per the mockup:
1. Header: "Claude" semibold + plan ("Max") in secondary, "···" button on the right.
2. A muted notice line under the header rule only when there is something to say (error,
   signed out).
3. Rings row: one 56px ring per limit in `usage.limits` (session, week, then models), 6px
   stroke, track in separator colour, arc in accent (red when `is_low`), the percent centred in
   SF Mono 13px semibold, the label under it 12px semibold, the reset caption under that in
   11px secondary (`Limit::reset_caption`). Rings are drawn with `gpui::canvas` +
   `PathBuilder::stroke` + `arc_to` (no SVG assets). Use `flex_grow` so N rings share the width.
4. Rule, then "Today" block: title row ("Today" semibold, the date "Sat, Sep 12" in 11px
   secondary on the right), a 3-column grid of stat tiles (15px SF Mono semibold number over an
   11px secondary label: sessions, tool calls, tokens via `compact_count`), then one row per
   model with ≥1% of today's tokens: name left, percent right in SF Mono secondary, and a 6px
   bar filled in `rgba(29,29,31,0.55)` light / `rgba(245,245,247,0.55)` dark.
5. Rule, then "Last 7 days" block: title row ("Last 7 days", "tokens per day"), seven bars
   28px tall max scaled to the busiest day (today in accent, the rest accent at 35%), and the
   weekday initials row under them in 10px tertiary.
6. Rule, then footer: "Updated 1:37 PM" left in tertiary; "Refresh" and "claude.ai" text
   buttons right (claude.ai opens `https://claude.ai/settings/usage` in the browser).
- "···" menu: Refresh; a "Menu bar shows" section header in 10px tertiary over one checkmark
  row per limit in the current snapshot ("Session", "Week", then each model — just Session and
  Week before the first fetch); a "Show percentage" checkmark row; Launch at login (SMAppService
  toggle, from `launch_at_login.rs`); rule; Quit Claudebar. Same look as mailbar's. Picking a row
  calls `UsageProvider::set_settings` and closes the menu. The popover is `overflow_hidden`, so
  `preferred_height()` grows to the menu's height while it is open.
- Keys: Esc closes (collapses the menu first), `r` refreshes.
- Light/dark follows the window appearance (`Theme::for_appearance`).
- `preferred_height()` adds up the sections so `main.rs` can size the window; the popover
  never scrolls.
- Signed out: instead of rings, one centred line "Sign in with `claude` in a terminal first"
  and the local blocks still render (they need no token).

## Crates
gpui 0.2, objc2 0.6 family (objc2-foundation, objc2-app-kit), chrono, reqwest (blocking,
rustls), serde/serde_json, keyring, dirs.

## Repo layout
- `src/main.rs` — app entry, activation policy, status item, popover window management (copy
  mailbar's shape: `StatusItem` click channel, `PopUp` window under the item, resize on notify)
- `src/model.rs` — `Usage`, `Limit`, `DayStats`, `LocalStats` (pure, unit-tested)
- `src/settings.rs` — the config file and its defaults (pure, unit-tested)
- `src/usage.rs` — Keychain read + the usage request + parsing (parsing unit-tested on the
  JSON above)
- `src/local.rs` — the session-log scan (unit-tested on a fixture written to a temp dir)
- `src/store_provider.rs` — `UsageProvider` over the two, with the background-executor +
  on-change hook pattern from mailbar
- `src/menu_bar_icon.rs` — the ring image; `src/status_item.rs` — Cocoa glue
- `src/ui/` — `popover.rs` root view, `rings.rs`, `stats.rs`, `provider.rs`, `theme.rs`
- `examples/popover_preview.rs` — the popover in a normal window over `StubProvider`;
  modes: `ready` (default), `nearly-out`, `signed-out`, `loading`, `error`, `menu` (ready with
  the "···" menu already down, since the preview takes no clicks)
- `scripts/bundle.sh` → `target/Claudebar.app`; `Makefile` — `make run`, `make install`,
  `make test`, `make check` (fmt + clippy `-D warnings`)

## Style
Comments explain why, in full sentences, the way daybar and mailbar do. No literal colours or
sizes in views. `cargo fmt`, clippy clean.
