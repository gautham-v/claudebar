# claudebar — spec

A macOS menu bar app that shows your Claude usage limits. The item draws one limit or several
side by side; click it and a popover drops down with a bar per limit (session, week, model) and
what you did today, read from the local Claude Code session logs. Built with GPUI (Rust). Sibling of daybar: same stack, same visual
language, same repo layout. MIT, open source.

## Reference
- `docs/mockup-popover-v2.dc.html` — the approved popover ("Native A", HTML/CSS). Its tokens
  were then tuned against the system Battery menu on a live screen, and `src/ui/theme.rs` is
  the source of truth: material `#ececee` at 85% over a blurred window (light; `#28282a` dark),
  text black at 85%, secondary `#6e6e73`, tertiary `#aeaeb2`, separators and bar tracks
  `rgba(0,0,0,0.09)`, a 6% rim, system red `#d70015` for a limit that is nearly spent — and
  nothing else with a hue. Popover **260px** wide, 10px radius, no gap under the menu bar.
  Font: system (`.SystemUIFont`); the Today values in `SF Mono`.
- `docs/mockup-popover.dc.html` — the first, card-style popover, superseded by v2 and kept for
  the record.
- `docs/mockup-menubar.dc.html` — the approved menu bar item: percentage then ring, the way the
  battery item sits, and the red state.

## Settings
`src/settings.rs`, a TOML file at `~/.config/claudebar/config.toml`
(`dirs::config_dir()/claudebar/config.toml`), written by the popover's Settings section and editable by hand:

| key | default | what it does |
|---|---|---|
| `menu_bar` | `["session"]` | which limits the menu bar item draws, in the order it draws them: any of `"session"`, `"weekly"`, or a model display name as the API spells it (`"Fable"`). A bare string still loads, as a list of one, so a 0.1.x file keeps working; what we write back is always a list |
| `show_percent` | `true` | `false` draws the rings alone, with no numbers |
| `show_labels` | `true` | the tag before each number (`Limit::menu_bar_tag`: `5h`, `wk`, or the model's name). Only ever drawn when the item is showing **more than one** limit — one number has nothing to be told apart from — so the default changes nothing for a single-limit item |
| `show_rings` | `true` | `false` draws the numbers alone |
| `low_remaining_percent` | `20.0` | how little of a window is left before it goes red |
| `refresh_minutes` | `5` | poll interval, read once at startup |

Every key has a serde default, so a partial file works and unknown keys are ignored.
`Settings::clamped` also refuses the two states that would leave the item with no ink and so no
click target: an empty `menu_bar` falls back to `["session"]`, and `show_percent` and
`show_rings` both off turns the rings back on.
`Settings::load() -> (Settings, Option<String>)`: defaults when the file is missing, and a
human-readable note when it is malformed, which the popover shows as its muted notice line
(never in place of a fetch error). `low_remaining_percent` is clamped to 0..=100 and
`refresh_minutes` to >= 1 on load. `Limit::is_low(low_remaining_percent)` takes the threshold;
`model::LOW_REMAINING_PERCENT` is only the default `Settings` starts from. `Settings` also owns
the two questions the views would otherwise each answer: `menu_bar_limits(&[Limit])` picks the
drawn limits out of a snapshot **in snapshot order**, skipping a pick the account has no limit
for, and `labels_shown(n)` is `show_labels && n > 1`. The provider seam carries `settings()` /
`set_settings()`; `StoreProvider` persists on set and then runs the on-change hook so the menu
bar item is redrawn. That hook goes through the foreground executor rather than being called
inline: `set_settings` is reached from a popover row's click handler, which already holds the
`App` borrow and the popover entity's lease, and touching either from there panics.

## Data
Two sources, both already on the machine. No account of its own; one optional config file.

1. **Limits** — `GET https://api.anthropic.com/api/oauth/usage` with
   `Authorization: Bearer <accessToken>` and `anthropic-beta: oauth-2025-04-20`. The token is the
   Claude Code OAuth token in the login Keychain: service `Claude Code-credentials`, account =
   the macOS username; the secret is JSON `{"claudeAiOauth": {"accessToken", "expiresAt" (ms),
   "subscriptionType" ("max"/"pro"), ...}}`. Read it by running `/usr/bin/security find-generic-password -w`, the same tool Claude Code writes it with, so the read survives token refreshes without a Keychain prompt.
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
   ourselves; Claude Code owns it. Poll every 5 minutes and whenever the popover opens, but never
   more than once a minute: the endpoint answers 429 when asked too often, which maps to
   `UsageError::RateLimited` ("Rate limited — will retry") and keeps the last good snapshot. That
   snapshot is also written to `~/Library/Caches/claudebar/usage.json` (`src/cache.rs`) and read
   back at launch, so the menu bar has a percentage before the first fetch lands.
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
- `NSStatusItem` whose button carries **one image and no title** (`NSCellImagePosition::ImageOnly`).
  Everything visible — tags, numbers and rings — is drawn into that image by
  `menu_bar_icon::item_image`, because a button has room for one title and one image and two
  limits need two rings. It also puts the gap between a number and its ring under our control
  rather than a space character's, and pins the font in one place: 11pt `menuBarFontOfSize`,
  which is what lines the digits up with the battery percentage next door.
- One group per limit in `settings.menu_bar`, in snapshot order: `tag`, number, ring. The tag is
  the same size drawn at 60% alpha — alpha, not a lighter colour, because alpha survives template
  tinting. Gaps: 3pt inside a group, 7pt between groups, so the item reads as groups rather than a
  row of loose numbers (`menu_bar_icon::place`, pure and unit-tested). While loading or signed
  out: one empty ring, no number, `appearsDisabled`.
- The ring: 13×13pt, drawn at runtime with Core Graphics (`src/menu_bar_icon.rs`): a track circle
  at 30% alpha, 1.8pt stroke, and an arc from 12 o'clock clockwise for that limit's percent, round
  caps.
- **Colour.** Normally the whole image is a template, so AppKit keeps only the alpha and tints it
  for the dark, light and reduced-transparency menu bars for free. A limit that `is_low` has to be
  really red, so one low limit takes the image out of template mode — and then every colour in it
  is ours: the low groups are `NSColor::systemRedColor`, the rest are the menu bar's own ink,
  white or black, read off the status item button's `effectiveAppearance` (`MenuBarInk`).
  `NSColor::labelColor` is not an option: inside a drawing handler it resolves against the *app's*
  appearance, which is light, and paints near-black numbers onto a dark menu bar. The ink is read
  when the image is built, so a theme or wallpaper change lands with the next fetch rather than
  instantly. No amber, no other colours. Redraw whenever a fetch lands.
- Click toggles the popover; clicking outside or pressing Esc closes it. No Dock icon.

## Popover (260 wide, height fits content)
Shaped like the system Battery menu, per `docs/mockup-popover-v2.dc.html`: 5px of inset all
round, 10px radius, a hairline border, plain rows at 3px/10px padding with a 6px-radius hover
wash, and separators inset 10px with 5px of margin. Monochrome — `Theme::warning` is the only
colour in the surface, and only on a limit bar that `is_low`. Top to bottom:
1. "Claude Usage" in 13px semibold. No plan name, no "···" button.
2. One block per limit in `usage.limits`: the label left and the percent right in secondary, a
   4px bar (track in the separator colour, fill in the primary text colour, or red when
   `Limit::is_low(low_remaining_percent)`), and an 11px secondary subtitle from
   `Limit::reset_sentence` — "Resets at 6:29 PM", "Resets Wed at 7:59 PM".
3. Separator, then "Today" in 13px semibold over three key-value rows — "Sessions 14",
   "Tool calls 412", "Tokens 1.1B" (`compact_count`) — the values right-aligned, secondary, in
   `SF Mono` so they read as tabular numerals. No per-model bars.
4. Separator, then one row: "Last 7 days" left, a seven-bar sparkline right (8px wide, 3px
   apart, 16px tall at the busiest day; today in the text colour, the six before it at 18%).
5. Separator, then the menu rows in 13px: "Refresh", "Open claude.ai"
   (`https://claude.ai/settings/usage`), "Settings", "Launch at login" (checkmark on the right,
   `SMAppService`, disabled outside a bundle with the reason under it), a separator, and
   "Quit Claudebar".
- "Settings" is a disclosure row: clicking it expands, indented and in place, a 10px tertiary
  "Menu bar shows" label, one checkmark row per limit in the current snapshot ("Session",
  "Week", then each model — just Session and Week before the first fetch), then the rows for how
  they are drawn: "Show percentage", "Show labels" and "Show rings". Picking any of them calls
  `UsageProvider::set_settings` and collapses the section.
- The limit rows are a **pick-any**, not a pick-one: several can be checked, and the item draws
  them in snapshot order. Two rules keep the item from ending up with nothing in it, and both are
  expressed as a row that keeps its checkmark but is drawn disabled rather than a click that
  silently does nothing: the last checked limit cannot be unchecked, and neither can the last of
  "Show percentage" / "Show rings".
- "Show labels" is only there when more than one limit is checked. With one number there is
  nothing to tell apart, so the question has no answer worth asking.
- States: signed out replaces the limit blocks with one 12px secondary line, "Sign in with
  `claude` in a terminal first"; before the first fetch lands, "Checking your limits…"; a fetch
  error or a malformed config file is one 11px secondary line under the header. The Today and
  Last 7 days blocks always render — they need no token.
- Keys: Esc collapses the Settings section, then closes; `r` refreshes.
- Light/dark follows the window appearance (`Theme::for_appearance`).
- `preferred_height()` adds up the sections — including the expanded Settings section — so
  `main.rs` can size the window; the popover never scrolls.

## Crates
gpui 0.2, objc2 0.6 family (objc2-foundation, objc2-app-kit), chrono, reqwest (blocking,
rustls), serde/serde_json, dirs.

## Repo layout
- `src/main.rs` — app entry, activation policy, status item, popover window management (copy
  daybar's shape: `StatusItem` click channel, `PopUp` window under the item, resize on notify)
- `src/model.rs` — `Usage`, `Limit`, `DayStats`, `LocalStats` (pure, unit-tested)
- `src/settings.rs` — the config file and its defaults (pure, unit-tested)
- `src/usage.rs` — Keychain read + the usage request + parsing (parsing unit-tested on the
  JSON above)
- `src/local.rs` — the session-log scan (unit-tested on a fixture written to a temp dir)
- `src/store_provider.rs` — `UsageProvider` over the two, with the background-executor +
  on-change hook pattern from daybar
- `src/menu_bar_icon.rs` — the item image: layout, text runs and the rings;
  `src/status_item.rs` — Cocoa glue
- `src/ui/` — `popover.rs` root view, `stats.rs`, `provider.rs`, `theme.rs`
- `examples/popover_preview.rs` — the popover in a normal window over `StubProvider`;
  modes: `ready` (default), `nearly-out`, `signed-out`, `loading`, `error`, `settings` (ready
  with the Settings section already expanded, since the preview takes no clicks)
- `scripts/bundle.sh` → `target/Claudebar.app`; `Makefile` — `make run`, `make install`,
  `make test`, `make check` (fmt + clippy `-D warnings`)

## Style
Comments explain why, in full sentences, the way daybar does. No literal colours or
sizes in views. `cargo fmt`, clippy clean.
