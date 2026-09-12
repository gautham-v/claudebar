//! The data the popover and the menu bar item render. Pure types plus a few
//! helpers; nothing here touches the network, the Keychain or the disk.

use chrono::{DateTime, Local, NaiveDate, Utc};

/// How little of a window has to be left before it is drawn in red, by
/// default — the same rule as the battery item, which goes red with 20% left.
/// [`crate::settings::Settings`] starts here and the user can move it.
pub const LOW_REMAINING_PERCENT: f32 = 20.0;

/// Which limit a ring stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LimitKind {
    /// The rolling 5-hour session window. This is what the menu bar shows.
    Session,
    /// The 7-day window across all models.
    Weekly,
    /// A 7-day window scoped to one model; the display name is the label.
    Model(String),
}

/// One rate limit as the API reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct Limit {
    pub kind: LimitKind,
    /// 0..=100, how much of the window has been used.
    pub percent: f32,
    /// When the window resets, if the API said.
    pub resets_at: Option<DateTime<Utc>>,
}

impl Limit {
    /// The ring's caption: "Session", "Week", or the model's display name.
    pub fn label(&self) -> &str {
        match &self.kind {
            LimitKind::Session => "Session",
            LimitKind::Weekly => "Week",
            LimitKind::Model(name) => name,
        }
    }

    /// Whether this limit should be drawn in red: less than
    /// `low_remaining_percent` of the window is left. The threshold is passed
    /// in rather than read from a const because it is a setting; the const is
    /// only the default the settings start from.
    pub fn is_low(&self, low_remaining_percent: f32) -> bool {
        self.percent >= 100.0 - low_remaining_percent
    }

    /// The reset time as the mockup shows it: "6:29 PM" when it is today,
    /// "Wed 7:59 PM" otherwise. `None` when the API gave no reset.
    pub fn reset_caption(&self, now: DateTime<Local>) -> Option<String> {
        let at = self.resets_at?.with_timezone(&Local);
        Some(if at.date_naive() == now.date_naive() {
            at.format("%-I:%M %p").to_string()
        } else {
            at.format("%a %-I:%M %p").to_string()
        })
    }

    /// The whole-number percent the menu bar and rings print.
    pub fn percent_rounded(&self) -> u32 {
        self.percent.round().clamp(0.0, 100.0) as u32
    }
}

/// A fetched snapshot of the account's limits.
#[derive(Debug, Clone, PartialEq)]
pub struct Usage {
    /// In display order: session, weekly, then any model-scoped limits.
    pub limits: Vec<Limit>,
    /// The plan name for the header ("Max", "Pro"), if known.
    pub plan: Option<String>,
    pub fetched_at: DateTime<Local>,
}

impl Usage {
    pub fn session(&self) -> Option<&Limit> {
        self.limits.iter().find(|l| l.kind == LimitKind::Session)
    }
}

/// What the local session logs say about one day.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DayStats {
    pub date: NaiveDate,
    /// Distinct session ids that had at least one assistant message.
    pub sessions: usize,
    /// `tool_use` content blocks across all assistant messages.
    pub tool_calls: usize,
    /// Input + output + cache read + cache creation, across all models.
    pub tokens: u64,
    /// Tokens per model id, largest first.
    pub tokens_by_model: Vec<(String, u64)>,
}

/// Today plus the six days before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalStats {
    /// Seven entries, oldest first, the last one being today. Days with no
    /// activity are present with zeros.
    pub days: Vec<DayStats>,
}

impl LocalStats {
    pub fn today(&self) -> Option<&DayStats> {
        self.days.last()
    }
}

/// A model id as the API spells it, shortened for a label: "claude-opus-5" →
/// "Opus", "claude-fable-5-1" → "Fable", "claude-haiku-4-5-20251001" → "Haiku".
pub fn model_display_name(id: &str) -> String {
    let rest = id.strip_prefix("claude-").unwrap_or(id);
    let family = rest.split('-').next().unwrap_or(rest);
    let mut chars = family.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => id.to_string(),
    }
}

/// "1.1B", "412M", "8.2K", "950" — the compact token count on a stat tile.
pub fn compact_count(n: u64) -> String {
    const UNITS: [(u64, &str); 3] = [(1_000_000_000, "B"), (1_000_000, "M"), (1_000, "K")];
    for (scale, suffix) in UNITS {
        if n >= scale {
            let value = n as f64 / scale as f64;
            return if value < 10.0 {
                format!("{value:.1}{suffix}")
            } else {
                format!("{}{suffix}", value.round() as u64)
            };
        }
    }
    n.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn limit(percent: f32) -> Limit {
        Limit {
            kind: LimitKind::Session,
            percent,
            resets_at: None,
        }
    }

    #[test]
    fn red_starts_with_twenty_percent_left() {
        assert!(!limit(79.6).is_low(LOW_REMAINING_PERCENT));
        assert!(limit(80.0).is_low(LOW_REMAINING_PERCENT));
        assert!(limit(100.0).is_low(LOW_REMAINING_PERCENT));
    }

    #[test]
    fn a_wider_threshold_goes_red_sooner() {
        assert!(limit(60.0).is_low(40.0));
        assert!(!limit(60.0).is_low(20.0));
        // A threshold of zero only fires on a window that is entirely spent.
        assert!(!limit(99.0).is_low(0.0));
        assert!(limit(100.0).is_low(0.0));
    }

    #[test]
    fn labels_follow_the_mockup() {
        assert_eq!(limit(0.0).label(), "Session");
        let week = Limit {
            kind: LimitKind::Weekly,
            ..limit(0.0)
        };
        assert_eq!(week.label(), "Week");
        let fable = Limit {
            kind: LimitKind::Model("Fable".into()),
            ..limit(0.0)
        };
        assert_eq!(fable.label(), "Fable");
    }

    #[test]
    fn reset_caption_drops_the_weekday_for_today() {
        let now = Local.with_ymd_and_hms(2026, 9, 12, 13, 37, 0).unwrap();
        let today = Local.with_ymd_and_hms(2026, 9, 12, 18, 29, 0).unwrap();
        let later = Local.with_ymd_and_hms(2026, 9, 16, 19, 59, 0).unwrap();
        let l = Limit {
            resets_at: Some(today.with_timezone(&Utc)),
            ..limit(2.0)
        };
        assert_eq!(l.reset_caption(now).as_deref(), Some("6:29 PM"));
        let l = Limit {
            resets_at: Some(later.with_timezone(&Utc)),
            ..limit(18.0)
        };
        assert_eq!(l.reset_caption(now).as_deref(), Some("Wed 7:59 PM"));
        assert_eq!(limit(1.0).reset_caption(now), None);
    }

    #[test]
    fn model_names_are_the_family_capitalised() {
        assert_eq!(model_display_name("claude-opus-5"), "Opus");
        assert_eq!(model_display_name("claude-fable-5-1"), "Fable");
        assert_eq!(model_display_name("claude-haiku-4-5-20251001"), "Haiku");
        assert_eq!(model_display_name("mystery"), "Mystery");
    }

    #[test]
    fn counts_compact_like_the_mockup() {
        assert_eq!(compact_count(950), "950");
        assert_eq!(compact_count(8_200), "8.2K");
        assert_eq!(compact_count(412_000_000), "412M");
        assert_eq!(compact_count(1_130_000_000), "1.1B");
    }
}
