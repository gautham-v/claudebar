# claudebar

Your Claude usage limits in the macOS menu bar. Rust + [GPUI](https://www.gpui.rs/), sibling of
[daybar](https://github.com/gautham-v/daybar).

<img src="docs/screenshot.png" width="480" alt="claudebar: the menu bar item and its popover">

The menu bar shows how much of your five-hour session you have used, and goes red when 20% is
left. Pick more than one limit and it draws them side by side — `5h 9% ◯  wk 45% ◐` — tagging
each so you can tell them apart. Click it for every limit on your plan with its reset time, what
you did today, and the last seven days. No account, no config, no Dock icon.

## Install

```sh
brew install --cask gautham-v/tap/claudebar
```

Then turn on **Launch at login** from the popover. The build is signed and notarized. The same
`Claudebar-<version>.zip` is on the [releases page](https://github.com/gautham-v/claudebar/releases)
if you would rather skip Homebrew.

To upgrade, run this, then quit and relaunch Claudebar:

```sh
brew upgrade --cask gautham-v/tap/claudebar
```

From source, with Rust and Xcode installed:

```sh
make install   # builds Claudebar.app, copies it to /Applications, launches it
```

`make run` builds and launches from `target/` instead; `cargo run --example popover_preview`
shows the popover over stub data.

## Settings

The **Settings** row in the popover, or `~/.config/claudebar/config.toml`:

| key | default | |
|---|---|---|
| `menu_bar` | `["session"]` | what the menu bar draws: any of `"session"`, `"weekly"`, or a model name like `"Fable"` — one name or a list, in the order drawn |
| `show_percent` | `true` | `false` shows the rings alone |
| `show_labels` | `true` | the `5h` / `wk` / model tag before each number; only ever drawn when there is more than one |
| `show_rings` | `true` | `false` shows the numbers alone, the narrowest way to carry three |
| `low_remaining_percent` | `20.0` | how much of a window is left when it goes red |
| `refresh_minutes` | `5` | how often to refetch |

## Where the numbers come from

- **Limits**: the same endpoint Claude Code's `/usage` calls, with the OAuth token Claude Code
  keeps in your Keychain. claudebar only reads that token. If it has expired, run `claude`.
- **Today and the last 7 days**: the Claude Code session logs in `~/.claude/projects`, read on
  your machine.

That one request is the only network traffic. Nothing from the logs leaves the machine, and there
is no telemetry. claudebar writes `~/.config/claudebar/config.toml` and a cache of the last
limits it fetched in `~/Library/Caches/claudebar`.

## License

MIT.
