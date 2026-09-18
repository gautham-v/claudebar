//! The handful of user settings, and the TOML file they live in.
//!
//! claudebar works with no config at all: everything here has a default, and a
//! missing file is the normal case rather than an error. The file exists so the
//! choices made in the popover's Settings section — which limits the menu bar
//! tracks, how it draws them, and the two thresholds — survive a relaunch.
//!
//! The file is `~/.config/claudebar/config.toml`, next to mailbar's own config
//! directory. It is read once at startup and rewritten whenever the menu
//! changes something; a partial file is fine (serde fills the rest in from the
//! defaults) and unknown keys are ignored, so a file written by a newer build
//! never stops an older one from starting. A file that does not parse at all is
//! not fatal either: [`Settings::load`] hands back the defaults plus one
//! human-readable note, which the popover shows as its muted notice line the
//! way mailbar shows a config complaint.

use std::path::PathBuf;

use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

use crate::model::{Limit, LimitKind, LOW_REMAINING_PERCENT};

/// Which limit the menu bar item tracks.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MenuBarLimit {
    /// The rolling five-hour window — what the menu bar has always shown.
    #[default]
    Session,
    /// The seven-day window across all models.
    Weekly,
    /// One model-scoped weekly window, named the way the API spells it
    /// ("Fable"), because that name is all the response gives us to match on.
    Model(String),
}

/// The value written to and read from the `menu_bar` key: a plain string, so
/// the file stays something a person can edit.
const SESSION_KEY: &str = "session";
const WEEKLY_KEY: &str = "weekly";

impl MenuBarLimit {
    /// How this choice is spelled in the config file.
    pub fn as_config_str(&self) -> &str {
        match self {
            MenuBarLimit::Session => SESSION_KEY,
            MenuBarLimit::Weekly => WEEKLY_KEY,
            MenuBarLimit::Model(name) => name,
        }
    }

    /// The other direction. Anything that is not one of the two window names is
    /// a model display name; the two names are matched case-insensitively so a
    /// hand-edited "Session" still works.
    pub fn parse(value: &str) -> Self {
        let trimmed = value.trim();
        if trimmed.eq_ignore_ascii_case(SESSION_KEY) {
            MenuBarLimit::Session
        } else if trimmed.eq_ignore_ascii_case(WEEKLY_KEY) {
            MenuBarLimit::Weekly
        } else {
            MenuBarLimit::Model(trimmed.to_string())
        }
    }

    /// Whether a limit from a snapshot is the one this setting names. Models
    /// are matched on the display name, which is the only identity the usage
    /// response carries for them.
    pub fn matches(&self, limit: &Limit) -> bool {
        match (self, &limit.kind) {
            (MenuBarLimit::Session, LimitKind::Session) => true,
            (MenuBarLimit::Weekly, LimitKind::Weekly) => true,
            (MenuBarLimit::Model(wanted), LimitKind::Model(name)) => wanted == name,
            _ => false,
        }
    }
}

impl Serialize for MenuBarLimit {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_config_str())
    }
}

impl<'de> Deserialize<'de> for MenuBarLimit {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        if value.trim().is_empty() {
            return Err(de::Error::custom("menu_bar must not be empty"));
        }
        Ok(MenuBarLimit::parse(&value))
    }
}

/// `menu_bar` reads as either one name or a list of them.
///
/// The key started life as a single string, and files written by 0.1.x still
/// have one; a list is what a build that can draw several limits writes. Both
/// load, and what we write back is always a list, because that is the shape
/// the setting has now.
mod menu_bar_list {
    use super::MenuBarLimit;
    use serde::de::Deserializer;
    use serde::ser::Serializer;
    use serde::Deserialize;

    pub fn serialize<S: Serializer>(
        limits: &[MenuBarLimit],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(limits)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<MenuBarLimit>, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum OneOrMany {
            One(MenuBarLimit),
            Many(Vec<MenuBarLimit>),
        }
        Ok(match OneOrMany::deserialize(deserializer)? {
            OneOrMany::One(limit) => vec![limit],
            OneOrMany::Many(limits) => limits,
        })
    }
}

/// Everything the user can choose. Every field carries a serde default, so a
/// file with one key in it is a valid file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Which limits the menu bar item draws, in the order it draws them.
    /// Several at once is the point of the list: session and week side by
    /// side is what most people came for.
    #[serde(with = "menu_bar_list")]
    pub menu_bar: Vec<MenuBarLimit>,
    /// Whether the menu bar prints the percentage. With this off the item is
    /// rings alone.
    pub show_percent: bool,
    /// Whether each number is prefixed with its window's tag ("5h", "wk", the
    /// model's name). Only ever drawn when the item is showing more than one
    /// limit: one number needs no saying which it is.
    pub show_labels: bool,
    /// Whether each limit gets its ring. With this off the item is numbers
    /// alone, which is the narrowest way to carry three windows.
    pub show_rings: bool,
    /// How little of a window has to be left before it is drawn in red.
    pub low_remaining_percent: f32,
    /// How often the limits and the local scan are refetched.
    pub refresh_minutes: u64,
}

/// The default poll interval: the usage endpoint is cheap and the numbers move
/// slowly, so five minutes keeps the menu bar honest without hammering it.
pub const DEFAULT_REFRESH_MINUTES: u64 = 5;

impl Default for Settings {
    fn default() -> Self {
        Self {
            menu_bar: vec![MenuBarLimit::Session],
            show_percent: true,
            show_labels: true,
            show_rings: true,
            low_remaining_percent: LOW_REMAINING_PERCENT,
            refresh_minutes: DEFAULT_REFRESH_MINUTES,
        }
    }
}

/// What goes at the top of a file we write, so whoever opens it knows what it
/// is and that the popover owns it.
const FILE_HEADER: &str = "\
# claudebar settings. Written by the Settings section; safe to edit by hand.
# menu_bar: any of \"session\", \"weekly\", or a model display name such as
# \"Fable\" \u{2014} one name or a list of them, drawn in the order given.
";

impl Settings {
    /// `~/.config/claudebar/config.toml`, or `None` on the odd machine with no
    /// home directory. It is spelled out from the home directory rather than
    /// taken from `dirs::config_dir()`, which on macOS is
    /// `~/Library/Application Support`: this is a hand-editable dotfile, and
    /// mailbar keeps its own config in `~/.config` for the same reason.
    pub fn path() -> Option<PathBuf> {
        Some(
            dirs::home_dir()?
                .join(".config")
                .join("claudebar")
                .join("config.toml"),
        )
    }

    /// Read the file. A missing file (or no config directory) gives the
    /// defaults and no note; a file that does not parse gives the defaults and
    /// one line the popover can show, because refusing to start over a stray
    /// character would be worse than ignoring it.
    pub fn load() -> (Settings, Option<String>) {
        let Some(path) = Self::path() else {
            return (Settings::default(), None);
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return (Settings::default(), None)
            }
            Err(error) => {
                return (
                    Settings::default(),
                    Some(format!("Could not read config.toml: {error}")),
                )
            }
        };
        match toml::from_str::<Settings>(&text) {
            Ok(settings) => (settings.clamped(), None),
            Err(error) => (
                Settings::default(),
                Some(format!(
                    "config.toml ignored: {}",
                    first_line(&error.to_string())
                )),
            ),
        }
    }

    /// Write the file, creating `~/.config/claudebar` if it is not there yet.
    /// The error is a sentence rather than a type because its only consumer is
    /// the popover's notice line.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or_else(|| "No config directory on this machine".to_string())?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
        }
        let body = toml::to_string(self).map_err(|e| format!("Could not write settings: {e}"))?;
        std::fs::write(&path, format!("{FILE_HEADER}{body}"))
            .map_err(|e| format!("Could not write {}: {e}", path.display()))
    }

    /// The same settings pulled back into the range that draws something, so
    /// a hand-edited file cannot produce a ring that never goes red, a refresh
    /// loop with no delay in it, or a menu bar item with nothing in it at all.
    fn clamped(mut self) -> Self {
        self.low_remaining_percent = self.low_remaining_percent.clamp(0.0, 100.0);
        self.refresh_minutes = self.refresh_minutes.max(1);
        // An empty list, or numbers and rings both off, leaves an item with no
        // ink and so no click target; fall back to the ring rather than to
        // nothing.
        if self.menu_bar.is_empty() {
            self.menu_bar = vec![MenuBarLimit::Session];
        }
        if !self.show_percent && !self.show_rings {
            self.show_rings = true;
        }
        self
    }

    /// The limits from a snapshot that the menu bar is set to draw, in the
    /// order the *settings* name them rather than the order the response
    /// happened to arrive in — a person who checks Week and then Session
    /// means the list they see in the popover, which is snapshot order, so
    /// this walks the snapshot and keeps what is picked.
    pub fn menu_bar_limits<'a>(&self, limits: &'a [Limit]) -> Vec<&'a Limit> {
        limits
            .iter()
            .filter(|limit| self.menu_bar.iter().any(|choice| choice.matches(limit)))
            .collect()
    }

    /// Whether the menu bar is set to draw this choice.
    pub fn shows(&self, choice: &MenuBarLimit) -> bool {
        self.menu_bar.contains(choice)
    }

    /// Whether a tag is drawn before each number: only when the user asked for
    /// labels *and* there is more than one number to tell apart.
    pub fn labels_shown(&self, drawn_limits: usize) -> bool {
        self.show_labels && drawn_limits > 1
    }
}

/// TOML's parse errors are several lines with a caret diagram under them; the
/// notice line has room for the first one only.
fn first_line(message: &str) -> String {
    message.lines().next().unwrap_or(message).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_behaviour_claudebar_shipped_with() {
        let settings = Settings::default();
        assert_eq!(settings.menu_bar, vec![MenuBarLimit::Session]);
        assert!(settings.show_percent);
        assert!(settings.show_labels);
        assert!(settings.show_rings);
        assert_eq!(settings.low_remaining_percent, LOW_REMAINING_PERCENT);
        assert_eq!(settings.refresh_minutes, 5);
    }

    #[test]
    fn menu_bar_round_trips_through_its_string() {
        for limit in [
            MenuBarLimit::Session,
            MenuBarLimit::Weekly,
            MenuBarLimit::Model("Fable".into()),
        ] {
            assert_eq!(MenuBarLimit::parse(limit.as_config_str()), limit);
        }
        assert_eq!(MenuBarLimit::Model("Fable".into()).as_config_str(), "Fable");
    }

    #[test]
    fn window_names_parse_whatever_their_case() {
        assert_eq!(MenuBarLimit::parse("Session"), MenuBarLimit::Session);
        assert_eq!(MenuBarLimit::parse(" WEEKLY "), MenuBarLimit::Weekly);
        assert_eq!(
            MenuBarLimit::parse("Opus"),
            MenuBarLimit::Model("Opus".into())
        );
    }

    #[test]
    fn a_partial_file_keeps_the_other_defaults() {
        let settings: Settings = toml::from_str("show_percent = false\n").unwrap();
        assert!(!settings.show_percent);
        assert_eq!(settings.menu_bar, vec![MenuBarLimit::Session]);
        assert_eq!(settings.refresh_minutes, 5);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let settings: Settings = toml::from_str("menu_bar = \"weekly\"\nfuture_key = 3\n").unwrap();
        assert_eq!(settings.menu_bar, vec![MenuBarLimit::Weekly]);
    }

    #[test]
    fn a_written_file_reads_back_as_itself() {
        let settings = Settings {
            menu_bar: vec![MenuBarLimit::Weekly, MenuBarLimit::Model("Fable".into())],
            show_percent: false,
            show_labels: false,
            show_rings: true,
            low_remaining_percent: 35.0,
            refresh_minutes: 12,
        };
        let text = toml::to_string(&settings).unwrap();
        assert!(text.contains("menu_bar = [\"weekly\", \"Fable\"]"));
        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), settings);
    }

    #[test]
    fn out_of_range_numbers_are_clamped() {
        let settings = Settings {
            low_remaining_percent: 480.0,
            refresh_minutes: 0,
            ..Settings::default()
        }
        .clamped();
        assert_eq!(settings.low_remaining_percent, 100.0);
        assert_eq!(settings.refresh_minutes, 1);
        let settings = Settings {
            low_remaining_percent: -5.0,
            ..Settings::default()
        }
        .clamped();
        assert_eq!(settings.low_remaining_percent, 0.0);
    }

    #[test]
    fn a_setting_matches_the_limit_it_names() {
        let limit = |kind| Limit {
            kind,
            percent: 0.0,
            resets_at: None,
        };
        assert!(MenuBarLimit::Session.matches(&limit(LimitKind::Session)));
        assert!(!MenuBarLimit::Session.matches(&limit(LimitKind::Weekly)));
        assert!(
            MenuBarLimit::Model("Fable".into()).matches(&limit(LimitKind::Model("Fable".into())))
        );
        assert!(
            !MenuBarLimit::Model("Fable".into()).matches(&limit(LimitKind::Model("Opus".into())))
        );
    }

    /// The key was a single string in 0.1.x and files still say so; it has to
    /// keep loading, as a list of one.
    #[test]
    fn a_single_name_still_loads_as_a_list_of_one() {
        let settings: Settings = toml::from_str("menu_bar = \"weekly\"\n").unwrap();
        assert_eq!(settings.menu_bar, vec![MenuBarLimit::Weekly]);
    }

    #[test]
    fn a_list_loads_in_the_order_it_is_written() {
        let settings: Settings =
            toml::from_str("menu_bar = [\"session\", \"weekly\", \"Fable\"]\n").unwrap();
        assert_eq!(
            settings.menu_bar,
            vec![
                MenuBarLimit::Session,
                MenuBarLimit::Weekly,
                MenuBarLimit::Model("Fable".into())
            ]
        );
    }

    /// A hand-edited file must not be able to leave the menu bar item with
    /// nothing to draw: no limits, or numbers and rings both off.
    #[test]
    fn an_item_with_no_ink_is_clamped_back_to_something() {
        let empty = Settings {
            menu_bar: vec![],
            ..Settings::default()
        }
        .clamped();
        assert_eq!(empty.menu_bar, vec![MenuBarLimit::Session]);

        let blank = Settings {
            show_percent: false,
            show_rings: false,
            ..Settings::default()
        }
        .clamped();
        assert!(blank.show_rings);
        // Turning the number off is still allowed on its own.
        let rings_only = Settings {
            show_percent: false,
            ..Settings::default()
        }
        .clamped();
        assert!(!rings_only.show_percent);
        assert!(rings_only.show_rings);
    }

    /// The menu bar draws the picked limits in snapshot order, and skips a
    /// setting the account has no limit for.
    #[test]
    fn the_picked_limits_come_back_in_snapshot_order() {
        let limits = vec![
            Limit {
                kind: LimitKind::Session,
                percent: 5.0,
                resets_at: None,
            },
            Limit {
                kind: LimitKind::Weekly,
                percent: 45.0,
                resets_at: None,
            },
            Limit {
                kind: LimitKind::Model("Fable".into()),
                percent: 52.0,
                resets_at: None,
            },
        ];
        let settings = Settings {
            // Written back to front, and naming a model the account does not
            // have.
            menu_bar: vec![
                MenuBarLimit::Model("Opus".into()),
                MenuBarLimit::Weekly,
                MenuBarLimit::Session,
            ],
            ..Settings::default()
        };
        let picked = settings.menu_bar_limits(&limits);
        assert_eq!(
            picked.iter().map(|l| l.label()).collect::<Vec<_>>(),
            vec!["Session", "Week"]
        );
    }

    /// One number needs no tag; two do. The setting only decides the second
    /// case.
    #[test]
    fn labels_are_only_drawn_when_there_is_something_to_tell_apart() {
        let on = Settings::default();
        assert!(!on.labels_shown(1));
        assert!(on.labels_shown(2));
        let off = Settings {
            show_labels: false,
            ..Settings::default()
        };
        assert!(!off.labels_shown(2));
    }

    #[test]
    fn a_malformed_file_becomes_one_line() {
        let error = toml::from_str::<Settings>("menu_bar = ").unwrap_err();
        let note = first_line(&error.to_string());
        assert!(!note.is_empty());
        assert!(!note.contains('\n'));
    }
}
