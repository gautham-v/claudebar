//! "Launch at login", via `SMAppService` (macOS 13+).
//!
//! `SMAppService.mainApp` registers *the running bundle's path* as a login
//! item, which has two consequences worth knowing: it only works when claudebar
//! is running from a `.app` (a bare `cargo run` binary has no bundle, so the
//! toggle reports [`Availability::NoBundle`] and stays disabled), and moving
//! the `.app` afterwards leaves a login item pointing at the old path — toggle
//! it off and on again after a move.

use objc2::rc::Retained;
use objc2_foundation::{NSBundle, NSError};
use objc2_service_management::{SMAppService, SMAppServiceStatus};

/// Whether the toggle can do anything at all in this process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// Running from a `.app`; the toggle works.
    Ready,
    /// Running as a bare binary (`cargo run`, an example): there is no bundle
    /// to register, so the toggle is shown disabled with a note.
    NoBundle,
}

/// Whether this process has a bundle identifier, which is what `SMAppService`
/// needs. `cargo run` and the examples do not.
pub fn availability() -> Availability {
    let has_identifier = NSBundle::mainBundle()
        .bundleIdentifier()
        .is_some_and(|id| !id.to_string().is_empty());
    if has_identifier {
        Availability::Ready
    } else {
        Availability::NoBundle
    }
}

/// The note shown beside a disabled toggle.
pub const NO_BUNDLE_NOTE: &str = "Only from the app bundle";

/// Whether the app is currently registered to start at login.
///
/// `RequiresApproval` counts as off: the registration exists but the user has
/// switched it off in System Settings, and the checkmark should agree with what
/// System Settings shows.
pub fn is_enabled() -> bool {
    if availability() != Availability::Ready {
        return false;
    }
    let service = unsafe { SMAppService::mainAppService() };
    unsafe { service.status() == SMAppServiceStatus::Enabled }
}

/// Turn launch-at-login on or off. The error is already phrased for display.
pub fn set_enabled(enabled: bool) -> Result<(), String> {
    if availability() != Availability::Ready {
        return Err(NO_BUNDLE_NOTE.to_string());
    }
    let service = unsafe { SMAppService::mainAppService() };
    let result = if enabled {
        unsafe { service.registerAndReturnError() }
    } else {
        unsafe { service.unregisterAndReturnError() }
    };
    result.map_err(|error| describe(&error, enabled))
}

fn describe(error: &Retained<NSError>, enabling: bool) -> String {
    let verb = if enabling { "enable" } else { "disable" };
    format!("Could not {verb}: {}", error.localizedDescription())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_test_binary_has_no_bundle_to_register() {
        // `cargo test` runs without a bundle identifier, so the toggle is off
        // and inert rather than reporting a spurious "enabled".
        assert_eq!(availability(), Availability::NoBundle);
        assert!(!is_enabled());
        assert_eq!(set_enabled(true), Err(NO_BUNDLE_NOTE.to_string()));
    }
}
