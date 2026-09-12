//! The seam between the views and the data layers.
//!
//! The popover never talks to `crate::usage` or `crate::local` directly: it
//! holds a [`UsageProvider`] and asks it for state and snapshots. That keeps
//! the whole UI renderable — and testable — without a network or a token,
//! through [`StubProvider`]. `store_provider.rs` implements the same trait over
//! the real layers.
//!
//! Every method takes `&self`: the popover holds the provider behind an `Rc`
//! and implementations use interior mutability.

use std::cell::RefCell;

use chrono::{DateTime, Duration, Local, Utc};

use crate::model::{DayStats, Limit, LimitKind, LocalStats, Usage};

/// What the popover should show above the rings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderState {
    /// No usable Claude Code credentials in the Keychain: the popover shows
    /// one line explaining how to get some ("Sign in with `claude` first").
    SignedOut,
    /// The first fetch is still out; nothing to draw yet.
    Loading,
    /// Normal operation.
    Ready,
    /// The last fetch failed; the message is shown as one muted line and the
    /// previous snapshot stays up.
    Error(String),
}

/// Everything the views need from the data layers.
pub trait UsageProvider {
    fn state(&self) -> ProviderState;
    /// The latest limits snapshot, if one has ever landed.
    fn usage(&self) -> Option<Usage>;
    /// Today and the six days before it, from the local session logs.
    fn local(&self) -> Option<LocalStats>;
    /// Refetch both now (popover open, the "···" menu's Refresh).
    fn refresh(&self);
    /// When the limits last landed — the footer's "Updated 1:37 PM".
    fn updated_at(&self) -> Option<DateTime<Local>>;
}

/// Fixture-backed provider: the numbers from the approved mockup.
pub struct StubProvider {
    inner: RefCell<Inner>,
}

struct Inner {
    state: ProviderState,
    usage: Option<Usage>,
    local: Option<LocalStats>,
}

impl Default for StubProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl StubProvider {
    /// Signed in, with the mockup's numbers.
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(Inner {
                state: ProviderState::Ready,
                usage: Some(fixture_usage()),
                local: Some(fixture_local()),
            }),
        }
    }

    /// No credentials: the popover shows the sign-in note.
    pub fn signed_out() -> Self {
        let this = Self::new();
        {
            let mut inner = this.inner.borrow_mut();
            inner.state = ProviderState::SignedOut;
            inner.usage = None;
        }
        this
    }

    /// First launch, fetch still out.
    pub fn loading() -> Self {
        let this = Self::new();
        {
            let mut inner = this.inner.borrow_mut();
            inner.state = ProviderState::Loading;
            inner.usage = None;
        }
        this
    }

    /// A fetch that failed after a good one: stale numbers plus an error line.
    pub fn with_error(message: &str) -> Self {
        let this = Self::new();
        this.inner.borrow_mut().state = ProviderState::Error(message.into());
        this
    }

    /// Every limit near the top, to see the red state.
    pub fn nearly_out() -> Self {
        let this = Self::new();
        {
            let mut inner = this.inner.borrow_mut();
            if let Some(usage) = &mut inner.usage {
                for (limit, percent) in usage.limits.iter_mut().zip([84.0, 91.0, 97.0]) {
                    limit.percent = percent;
                }
            }
        }
        this
    }
}

impl UsageProvider for StubProvider {
    fn state(&self) -> ProviderState {
        self.inner.borrow().state.clone()
    }
    fn usage(&self) -> Option<Usage> {
        self.inner.borrow().usage.clone()
    }
    fn local(&self) -> Option<LocalStats> {
        self.inner.borrow().local.clone()
    }
    fn refresh(&self) {}
    fn updated_at(&self) -> Option<DateTime<Local>> {
        self.inner.borrow().usage.as_ref().map(|u| u.fetched_at)
    }
}

/// The mockup's limits: session 2%, week 18%, Fable 22%.
pub fn fixture_usage() -> Usage {
    let now = Local::now();
    let session_reset = (now + Duration::hours(4) + Duration::minutes(52)).with_timezone(&Utc);
    let week_reset = (now + Duration::days(4) + Duration::hours(6)).with_timezone(&Utc);
    Usage {
        limits: vec![
            Limit {
                kind: LimitKind::Session,
                percent: 2.0,
                resets_at: Some(session_reset),
            },
            Limit {
                kind: LimitKind::Weekly,
                percent: 18.0,
                resets_at: Some(week_reset),
            },
            Limit {
                kind: LimitKind::Model("Fable".into()),
                percent: 22.0,
                resets_at: Some(week_reset),
            },
        ],
        plan: Some("Max".into()),
        fetched_at: now,
    }
}

/// The mockup's local stats: 14 sessions, 412 tool calls, 1.1B tokens today,
/// 86% Fable / 13% Opus, and a week of bars.
pub fn fixture_local() -> LocalStats {
    let today = Local::now().date_naive();
    let per_day: [u64; 7] = [
        540_000_000,
        860_000_000,
        350_000_000,
        1_100_000_000,
        700_000_000,
        230_000_000,
        1_130_000_000,
    ];
    let days = per_day
        .iter()
        .enumerate()
        .map(|(i, tokens)| {
            let date = today - Duration::days(6 - i as i64);
            let fable = tokens * 86 / 100;
            let opus = tokens * 13 / 100;
            DayStats {
                date,
                sessions: if i == 6 { 14 } else { 6 + i },
                tool_calls: if i == 6 { 412 } else { 120 + 40 * i },
                tokens: *tokens,
                tokens_by_model: vec![
                    ("claude-fable-5-1".into(), fable),
                    ("claude-opus-5".into(), opus),
                    ("claude-haiku-4-5-20251001".into(), tokens - fable - opus),
                ],
            }
        })
        .collect();
    LocalStats { days }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fixture_matches_the_mockup() {
        let usage = fixture_usage();
        assert_eq!(usage.session().map(|l| l.percent), Some(2.0));
        assert_eq!(usage.limits.len(), 3);
        let local = fixture_local();
        assert_eq!(local.days.len(), 7);
        let today = local.today().unwrap();
        assert_eq!(today.sessions, 14);
        assert_eq!(today.tool_calls, 412);
        assert_eq!(crate::model::compact_count(today.tokens), "1.1B");
    }

    #[test]
    fn stub_states_render_as_described() {
        assert_eq!(StubProvider::signed_out().state(), ProviderState::SignedOut);
        assert!(StubProvider::signed_out().usage().is_none());
        assert_eq!(StubProvider::loading().state(), ProviderState::Loading);
        assert!(StubProvider::nearly_out()
            .usage()
            .unwrap()
            .limits
            .iter()
            .all(|l| l.is_low()));
    }
}
