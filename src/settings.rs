//! The handful of user settings, and the TOML file they live in.
//!
//! claudebar works with no config at all: everything here has a default, and a
//! missing file is the normal case rather than an error. The file exists so the
//! choices made in the popover's "···" menu — which limit the menu bar tracks,
//! whether it prints a number, and the two thresholds — survive a relaunch.
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

/// Everything the user can choose. Every field carries a serde default, so a
/// file with one key in it is a valid file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Which limit the menu bar item tracks.
    pub menu_bar: MenuBarLimit,
    /// Whether the menu bar prints the percentage beside the ring. With this
    /// off the item is the ring alone.
    pub show_percent: bool,
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
            menu_bar: MenuBarLimit::Session,
            show_percent: true,
            low_remaining_percent: LOW_REMAINING_PERCENT,
            refresh_minutes: DEFAULT_REFRESH_MINUTES,
        }
    }
}

/// What goes at the top of a file we write, so whoever opens it knows what it
/// is and that the popover owns it.
const FILE_HEADER: &str = "\
# claudebar settings. Written by the \u{b7}\u{b7}\u{b7} menu; safe to edit by hand.
# menu_bar: \"session\", \"weekly\", or a model display name such as \"Fable\".
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

    /// The same settings with the two numbers pulled back into their usable
    /// range, so a hand-edited file cannot produce a ring that never goes red
    /// or a refresh loop with no delay in it.
    fn clamped(mut self) -> Self {
        self.low_remaining_percent = self.low_remaining_percent.clamp(0.0, 100.0);
        self.refresh_minutes = self.refresh_minutes.max(1);
        self
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
        assert_eq!(settings.menu_bar, MenuBarLimit::Session);
        assert!(settings.show_percent);
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
        assert_eq!(settings.menu_bar, MenuBarLimit::Session);
        assert_eq!(settings.refresh_minutes, 5);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let settings: Settings = toml::from_str("menu_bar = \"weekly\"\nfuture_key = 3\n").unwrap();
        assert_eq!(settings.menu_bar, MenuBarLimit::Weekly);
    }

    #[test]
    fn a_written_file_reads_back_as_itself() {
        let settings = Settings {
            menu_bar: MenuBarLimit::Model("Fable".into()),
            show_percent: false,
            low_remaining_percent: 35.0,
            refresh_minutes: 12,
        };
        let text = toml::to_string(&settings).unwrap();
        assert!(text.contains("menu_bar = \"Fable\""));
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

    #[test]
    fn a_malformed_file_becomes_one_line() {
        let error = toml::from_str::<Settings>("menu_bar = ").unwrap_err();
        let note = first_line(&error.to_string());
        assert!(!note.is_empty());
        assert!(!note.contains('\n'));
    }
}
