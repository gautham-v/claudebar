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

use chrono::{DateTime, Local};
use gpui::AsyncApp;

use crate::local;
use crate::model::{LocalStats, Usage};
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
}

impl StoreProvider {
    /// Build the provider. Nothing is fetched until [`Self::refresh`] runs, so
    /// this never blocks and never fails.
    pub fn new(cx: &AsyncApp) -> Self {
        Self {
            snapshot: Arc::new(Mutex::new(Snapshot::default())),
            cx: cx.clone(),
            on_change: RefCell::new(None),
            fetching: Arc::new(Mutex::new(false)),
        }
    }

    /// Install the main-thread hook run after every background task.
    pub fn set_on_change(&self, hook: OnChange) {
        *self.on_change.borrow_mut() = Some(hook);
    }

    /// What the menu bar item should draw: the session limit, or nothing at all
    /// while loading, signed out or errored without a previous snapshot.
    pub fn menu_bar_state(&self) -> MenuBarState {
        menu_bar_state_for(self.snapshot.lock().unwrap().usage.as_ref())
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
fn menu_bar_state_for(usage: Option<&Usage>) -> MenuBarState {
    match usage.and_then(|u| u.session()) {
        Some(session) => MenuBarState::Usage {
            percent: session.percent_rounded(),
            low: session.is_low(),
        },
        None => MenuBarState::Idle,
    }
}

impl UsageProvider for StoreProvider {
    fn state(&self) -> ProviderState {
        let snapshot = self.snapshot.lock().unwrap();
        match &snapshot.state {
            Some(state) => state.clone(),
            None => ProviderState::Loading,
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

    #[test]
    fn no_snapshot_leaves_the_menu_bar_idle() {
        assert_eq!(menu_bar_state_for(None), MenuBarState::Idle);
    }

    #[test]
    fn a_session_limit_becomes_a_rounded_percent() {
        assert_eq!(
            menu_bar_state_for(Some(&usage_with(2.4))),
            MenuBarState::Usage {
                percent: 2,
                low: false
            }
        );
    }

    #[test]
    fn a_nearly_spent_session_goes_red() {
        assert_eq!(
            menu_bar_state_for(Some(&usage_with(84.0))),
            MenuBarState::Usage {
                percent: 84,
                low: true
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
        assert_eq!(menu_bar_state_for(Some(&weekly_only)), MenuBarState::Idle);
    }
}
