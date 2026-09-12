//! The bridge between the views' [`UsageProvider`] and the two data layers.
//!
//! Both layers block: [`crate::usage::fetch`] does a Keychain read and an HTTP
//! round trip, and [`crate::local::scan_default`] walks a few hundred megabytes
//! of JSONL. The popover renders on the main thread, so every fetch here has
//! the same shape as mailbar's: hand the work to `cx.background_executor()`,
//! and when it lands hop back to the main thread and run the change hook (which
//! re-renders the popover and re-draws the menu bar item). Nothing in this file
//! blocks the UI.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// The least time between two usage fetches. The endpoint rate-limits, and
/// opening the popover a few times in a minute is not a reason to ask again.
const MIN_FETCH_GAP: std::time::Duration = std::time::Duration::from_secs(60);

use chrono::{DateTime, Local};
use gpui::AsyncApp;

use crate::local;
use crate::model::{LocalStats, Usage};
use crate::settings::Settings;
use crate::status_item::MenuBarState;
use crate::ui::provider::{ProviderState, UsageProvider};
use crate::usage::{self, UsageError};

/// Called on the main thread after any background task finishes.
pub type OnChange = Rc<dyn Fn(&mut gpui::App)>;

/// Everything a fetch writes and the views read. Behind a mutex because the
/// background executor's threads are the writers and the main thread is the
/// only reader.
#[derive(Default)]
struct Snapshot {
    state: Option<ProviderState>,
    /// The last limits that came back. Kept across a failed fetch, so a blip
    /// of network trouble shows stale rings plus a muted line rather than an
    /// empty popover.
    usage: Option<Usage>,
    local: Option<LocalStats>,
}

/// A [`UsageProvider`] over the real Keychain/HTTP and session-log layers.
pub struct StoreProvider {
    snapshot: Arc<Mutex<Snapshot>>,
    cx: AsyncApp,
    on_change: RefCell<Option<OnChange>>,
    /// A fetch is already out; a second popover open should not start another.
    fetching: Arc<Mutex<bool>>,
    /// When the last fetch was started. Every popover open asks for a
    /// refresh, and the usage endpoint answers 429 when asked too often, so
    /// opens closer together than [`MIN_FETCH_GAP`] reuse the snapshot.
    last_fetch: Arc<Mutex<Option<Instant>>>,
    /// The user's choices, read once at construction and rewritten whenever
    /// the "···" menu changes one.
    settings: RefCell<Settings>,
    /// A complaint about the config file — malformed on load, or unwritable on
    /// save. Shown as the popover's muted line, but never in place of a fetch
    /// error, which is the more urgent of the two.
    settings_note: RefCell<Option<String>>,
}

impl StoreProvider {
    /// Build the provider. Nothing is fetched until [`Self::refresh`] runs, so
    /// this never blocks and never fails. The settings file is small enough to
    /// read here on the main thread, and everything downstream — the first menu
    /// bar title included — needs it before the first fetch lands.
    pub fn new(cx: &AsyncApp) -> Self {
        let (settings, note) = Settings::load();
        // Start from the last good numbers, so the menu bar has a percentage
        // to show before the first fetch lands. The state stays unset: this is
        // still "loading" as far as the notice line is concerned.
        let snapshot = Snapshot {
            usage: crate::cache::load(),
            ..Snapshot::default()
        };
        Self {
            snapshot: Arc::new(Mutex::new(snapshot)),
            cx: cx.clone(),
            on_change: RefCell::new(None),
            fetching: Arc::new(Mutex::new(false)),
            last_fetch: Arc::new(Mutex::new(None)),
            settings: RefCell::new(settings),
            settings_note: RefCell::new(note),
        }
    }

    /// How often `main.rs` should refetch. Read once at startup: changing the
    /// interval is a config-file edit, and a relaunch to pick it up is a fair
    /// price for not having to restart the timer loop from a menu click.
    pub fn refresh_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.settings.borrow().refresh_minutes * 60)
    }

    /// Install the main-thread hook run after every background task.
    pub fn set_on_change(&self, hook: OnChange) {
        *self.on_change.borrow_mut() = Some(hook);
    }

    /// What the menu bar item should draw: whichever limit the settings name,
    /// or nothing at all while loading, signed out, errored without a previous
    /// snapshot, or when that limit is not in the snapshot at all.
    pub fn menu_bar_state(&self) -> MenuBarState {
        menu_bar_state_for(
            self.snapshot.lock().unwrap().usage.as_ref(),
            &self.settings.borrow(),
        )
    }

    /// Run both fetches off the main thread, then fire the change hook.
    fn fetch(&self) {
        // The guard is dropped before the work is spawned, so a refresh that
        // arrives while one is out is dropped rather than queued: the next
        // five-minute tick or popover open picks the new numbers up anyway.
        {
            let mut fetching = self.fetching.lock().unwrap();
            if *fetching {
                return;
            }
            let mut last = self.last_fetch.lock().unwrap();
            if last.is_some_and(|at| at.elapsed() < MIN_FETCH_GAP) {
                return;
            }
            *last = Some(Instant::now());
            *fetching = true;
        }
        let snapshot = self.snapshot.clone();
        let fetching = self.fetching.clone();
        let hook = self.on_change.borrow().clone();
        self.cx
            .spawn(async move |cx: &mut AsyncApp| {
                cx.background_executor()
                    .spawn(async move {
                        let limits = usage::fetch();
                        let scanned = local::scan_default(Local::now().date_naive());
                        let mut snapshot = snapshot.lock().unwrap();
                        snapshot.local = Some(scanned);
                        match limits {
                            Ok(usage) => {
                                crate::cache::save(&usage);
                                snapshot.usage = Some(usage);
                                snapshot.state = Some(ProviderState::Ready);
                            }
                            // Signed out is its own state: the popover swaps
                            // the rings for the sign-in line, and the local
                            // blocks still render because they need no token.
                            Err(UsageError::SignedOut) => {
                                snapshot.usage = None;
                                snapshot.state = Some(ProviderState::SignedOut);
                            }
                            // Anything else keeps the last good numbers and
                            // adds one muted line saying what went wrong.
                            Err(error) => {
                                snapshot.state = Some(ProviderState::Error(error.message()));
                            }
                        }
                        *fetching.lock().unwrap() = false;
                    })
                    .await;
                if let Some(hook) = hook {
                    let _ = cx.update(|cx| hook(cx));
                }
            })
            .detach();
    }
}

/// The menu bar item's state for a snapshot. A free function so the mapping —
/// the only part of this file that is pure — is exercised by the tests below
/// rather than reimplemented by them.
fn menu_bar_state_for(usage: Option<&Usage>, settings: &Settings) -> MenuBarState {
    let chosen = usage.and_then(|u| u.limits.iter().find(|l| settings.menu_bar.matches(l)));
    match chosen {
        Some(limit) => MenuBarState::Usage {
            percent: limit.percent_rounded(),
            low: limit.is_low(settings.low_remaining_percent),
            show_percent: settings.show_percent,
        },
        None => MenuBarState::Idle,
    }
}

impl UsageProvider for StoreProvider {
    fn state(&self) -> ProviderState {
        let snapshot = self.snapshot.lock().unwrap();
        let state = match &snapshot.state {
            Some(state) => state.clone(),
            None => ProviderState::Loading,
        };
        // A config complaint is worth one muted line, but only when the fetch
        // has nothing more urgent to say: an error or a signed-out state owns
        // that line, and being signed out changes the whole popover.
        match (&state, self.settings_note.borrow().clone()) {
            (ProviderState::Loading | ProviderState::Ready, Some(note)) => {
                ProviderState::Error(note)
            }
            _ => state,
        }
    }

    fn usage(&self) -> Option<Usage> {
        self.snapshot.lock().unwrap().usage.clone()
    }

    fn local(&self) -> Option<LocalStats> {
        self.snapshot.lock().unwrap().local.clone()
    }

    fn refresh(&self) {
        self.fetch();
    }

    fn updated_at(&self) -> Option<DateTime<Local>> {
        self.snapshot
            .lock()
            .unwrap()
            .usage
            .as_ref()
            .map(|u| u.fetched_at)
    }

    fn settings(&self) -> Settings {
        self.settings.borrow().clone()
    }

    /// Take the new settings, write them out, and run the change hook so the
    /// menu bar item is re-titled and the popover re-rendered. The settings are
    /// applied whether or not the write succeeds — refusing a menu click
    /// because a directory is read-only would be the wrong trade — and a failed
    /// write becomes the notice line. A successful one clears whatever the
    /// notice was saying about the file, including a load-time complaint that
    /// this write has just fixed.
    fn set_settings(&self, settings: Settings) {
        *self.settings.borrow_mut() = settings;
        *self.settings_note.borrow_mut() = self.settings.borrow().save().err();
        let hook = self.on_change.borrow().clone();
        if let Some(hook) = hook {
            let cx = self.cx.clone();
            let _ = cx.update(|cx| hook(cx));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Limit, LimitKind};

    fn usage_with(percent: f32) -> Usage {
        Usage {
            limits: vec![Limit {
                kind: LimitKind::Session,
                percent,
                resets_at: None,
            }],
            plan: Some("Max".into()),
            fetched_at: Local::now(),
        }
    }

    use crate::settings::MenuBarLimit;

    #[test]
    fn no_snapshot_leaves_the_menu_bar_idle() {
        assert_eq!(
            menu_bar_state_for(None, &Settings::default()),
            MenuBarState::Idle
        );
    }

    #[test]
    fn a_session_limit_becomes_a_rounded_percent() {
        assert_eq!(
            menu_bar_state_for(Some(&usage_with(2.4)), &Settings::default()),
            MenuBarState::Usage {
                percent: 2,
                low: false,
                show_percent: true
            }
        );
    }

    #[test]
    fn a_nearly_spent_session_goes_red() {
        assert_eq!(
            menu_bar_state_for(Some(&usage_with(84.0)), &Settings::default()),
            MenuBarState::Usage {
                percent: 84,
                low: true,
                show_percent: true
            }
        );
    }

    /// A snapshot with no session limit in it (a response that only carried the
    /// weekly window) has no percentage for the menu bar, and must not fall
    /// back to another limit's number.
    #[test]
    fn a_snapshot_without_a_session_limit_is_idle() {
        let weekly_only = Usage {
            limits: vec![Limit {
                kind: LimitKind::Weekly,
                percent: 92.0,
                resets_at: None,
            }],
            plan: None,
            fetched_at: Local::now(),
        };
        assert_eq!(
            menu_bar_state_for(Some(&weekly_only), &Settings::default()),
            MenuBarState::Idle
        );
    }

    /// The same snapshot, read through a setting that names the weekly window.
    #[test]
    fn the_settings_pick_which_limit_the_menu_bar_tracks() {
        let usage = Usage {
            limits: vec![
                Limit {
                    kind: LimitKind::Session,
                    percent: 2.0,
                    resets_at: None,
                },
                Limit {
                    kind: LimitKind::Weekly,
                    percent: 55.0,
                    resets_at: None,
                },
                Limit {
                    kind: LimitKind::Model("Fable".into()),
                    percent: 71.0,
                    resets_at: None,
                },
            ],
            plan: None,
            fetched_at: Local::now(),
        };
        let with = |menu_bar| {
            menu_bar_state_for(
                Some(&usage),
                &Settings {
                    menu_bar,
                    ..Settings::default()
                },
            )
        };
        assert!(matches!(
            with(MenuBarLimit::Weekly),
            MenuBarState::Usage { percent: 55, .. }
        ));
        assert!(matches!(
            with(MenuBarLimit::Model("Fable".into())),
            MenuBarState::Usage { percent: 71, .. }
        ));
        // A limit the account does not have leaves the item idle rather than
        // silently showing a different window's number.
        assert_eq!(with(MenuBarLimit::Model("Opus".into())), MenuBarState::Idle);
    }

    #[test]
    fn the_settings_carry_the_low_threshold_and_the_number() {
        let state = menu_bar_state_for(
            Some(&usage_with(65.0)),
            &Settings {
                low_remaining_percent: 40.0,
                show_percent: false,
                ..Settings::default()
            },
        );
        assert_eq!(
            state,
            MenuBarState::Usage {
                percent: 65,
                low: true,
                show_percent: false
            }
        );
    }
}
