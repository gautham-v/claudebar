//! All the Cocoa glue lives here: activation policy, the `NSStatusItem`, and
//! the bridge from an AppKit click back into gpui.
//!
//! The bridge is deliberately dumb: the status item's target/action is a tiny
//! `objc2` class that owns an unbounded channel sender and pushes a
//! [`StatusItemEvent`] on every click. `main.rs` drains that channel from a
//! gpui task, so nothing AppKit-shaped leaks into the views.

use block2::RcBlock;
use futures::channel::mpsc::{self, UnboundedReceiver, UnboundedSender};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use objc2::{define_class, msg_send, sel, AnyThread, DefinedClass, MainThreadMarker};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSCellImagePosition, NSColor, NSControl, NSEvent,
    NSEventMask, NSFont, NSForegroundColorAttributeName, NSScreen, NSStatusBar, NSStatusBarButton,
    NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::{NSAttributedString, NSDictionary, NSPoint, NSRect, NSString};

use crate::menu_bar_icon;

/// The gap between the percentage and the ring. AppKit only exposes image
/// padding on the button from macOS 14, and `objc2` 0.3 does not bind it, so
/// the spacing is a trailing space in the title instead — a space in the menu
/// bar font is about the 4pt this wants. It trails rather than leads because
/// the number comes first and the ring after it, the way the battery item sits.
const IMAGE_TITLE_GAP: &str = " ";

/// The menu bar title as AppKit is given it: the percentage with its trailing
/// gap, or nothing at all when there is no percentage to show.
fn spaced_title(title: &str) -> String {
    if title.is_empty() {
        String::new()
    } else {
        format!("{title}{IMAGE_TITLE_GAP}")
    }
}

/// What the menu bar item is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuBarState {
    /// Loading, signed out, or otherwise nothing to report: an empty ring, no
    /// number, and AppKit's own disabled rendering.
    Idle,
    /// A session utilisation. `low` is [`crate::model::Limit::is_low`], which
    /// turns both the number and the ring red.
    Usage { percent: u32, low: bool },
}

impl MenuBarState {
    /// The number beside the ring. Whole percent, as the spec asks; empty while
    /// idle so the ring stands alone.
    fn title(self) -> String {
        match self {
            MenuBarState::Idle => String::new(),
            MenuBarState::Usage { percent, .. } => format!("{percent}%"),
        }
    }

    /// The percentage the ring is drawn for.
    fn ring_percent(self) -> f32 {
        match self {
            MenuBarState::Idle => 0.0,
            MenuBarState::Usage { percent, .. } => percent as f32,
        }
    }

    /// Whether this state is the red, nearly-out one.
    fn low(self) -> bool {
        matches!(self, MenuBarState::Usage { low: true, .. })
    }

    /// Idle states are faded, the way the system items fade when they have
    /// nothing to say.
    fn dimmed(self) -> bool {
        matches!(self, MenuBarState::Idle)
    }
}

/// What the status item tells the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusItemEvent {
    /// The user clicked the menu bar item.
    Clicked,
    /// The user clicked somewhere outside this app; the popover should dismiss.
    ///
    /// A non-activating panel never makes claudebar the active app, so AppKit's
    /// resign-key notification is not a reliable "clicked outside" signal — a
    /// global event monitor is. Global monitors never see our own app's
    /// clicks, so clicking the status item itself does not produce this.
    ClickedOutside,
}

/// A rectangle in gpui's screen coordinate space (top-left origin, y down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

struct TargetIvars {
    tx: UnboundedSender<StatusItemEvent>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "ClaudebarStatusItemTarget"]
    #[ivars = TargetIvars]
    struct StatusItemTarget;

    unsafe impl NSObjectProtocol for StatusItemTarget {}

    impl StatusItemTarget {
        #[unsafe(method(claudebarStatusItemClicked:))]
        fn clicked(&self, _sender: *mut AnyObject) {
            // Unbounded send only fails once the receiver is gone, which means
            // the app is shutting down — dropping the click is correct then.
            let _ = self.ivars().tx.unbounded_send(StatusItemEvent::Clicked);
        }
    }
);

impl StatusItemTarget {
    fn new(tx: UnboundedSender<StatusItemEvent>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(TargetIvars { tx });
        unsafe { msg_send![super(this), init] }
    }
}

/// Owns the menu bar item for the lifetime of the app.
///
/// Dropping it removes the item from the menu bar, so keep it alive.
pub struct StatusItem {
    item: Retained<NSStatusItem>,
    // Held so the target outlives the status item's unretained `target` pointer.
    _target: Retained<StatusItemTarget>,
    // The global mouse-down monitor; removed on drop.
    outside_monitor: Option<Retained<objc2::runtime::AnyObject>>,
}

impl StatusItem {
    /// Install the menu bar item. Returns it plus the click channel.
    ///
    /// The item is the session percentage followed by the ring image, in the
    /// menu bar's own font, so it sits like the battery item. Callers pass the
    /// state they have; [`MenuBarState::Idle`] leaves the ring alone with no
    /// number, and it stays clickable because the image has a hit area of its
    /// own.
    pub fn new(
        mtm: MainThreadMarker,
        state: MenuBarState,
    ) -> (Self, UnboundedReceiver<StatusItemEvent>) {
        let (tx, rx) = mpsc::unbounded();
        let tx_outside = tx.clone();
        let target = StatusItemTarget::new(tx);

        let bar = NSStatusBar::systemStatusBar();
        let item = bar.statusItemWithLength(NSVariableStatusItemLength);

        if let Some(button) = item.button(mtm) {
            unsafe {
                // Number left, ring right; `imageHugsTitle` keeps them as a
                // pair instead of pushing the image to the button's far edge.
                button.setImagePosition(NSCellImagePosition::ImageTrailing);
                button.setImageHugsTitle(true);
                // The menu bar's own text metric, so the number matches the
                // system items next to it rather than the default control font.
                let font = NSFont::menuBarFontOfSize(0.0);
                button.setFont(Some(&font));

                let control: &NSControl = &button;
                control.setTarget(Some(&*target));
                control.setAction(Some(sel!(claudebarStatusItemClicked:)));
            }
            apply_state(&button, state);
        }

        let outside_monitor = install_outside_click_monitor(tx_outside);

        (
            Self {
                item,
                _target: target,
                outside_monitor,
            },
            rx,
        )
    }

    /// Redraw the item for a new state. Call this whenever a fetch lands: the
    /// ring is a bitmap for one particular percentage, so it is rebuilt rather
    /// than mutated.
    pub fn set_state(&self, mtm: MainThreadMarker, state: MenuBarState) {
        if let Some(button) = self.item.button(mtm) {
            apply_state(&button, state);
        }
    }

    /// The status item button's frame, in gpui screen coordinates.
    ///
    /// AppKit hands back a bottom-left-origin rect on the status bar's own
    /// window; gpui wants top-left-origin relative to the primary display, so
    /// we flip through the primary screen's height.
    pub fn screen_rect(&self, mtm: MainThreadMarker) -> Option<ScreenRect> {
        let button = self.item.button(mtm)?;
        let window = button.window()?;
        let frame: NSRect = window.frame();
        let flip_height = primary_screen_height(mtm)?;

        Some(ScreenRect {
            x: frame.origin.x as f32,
            // Top edge in flipped coords.
            y: (flip_height - (frame.origin.y + frame.size.height)) as f32,
            width: frame.size.width as f32,
            height: frame.size.height as f32,
        })
    }
}

/// Push a state onto the button: the number, the ring, and the fade.
///
/// The low state is the only one that needs an attributed title — AppKit has no
/// plain "title colour" on a status item button, so the red number is a
/// `NSForegroundColorAttributeName` run. Setting a plain title afterwards is
/// what clears it again; the two titles are separate properties and the
/// attributed one wins whenever it is set.
fn apply_state(button: &NSStatusBarButton, state: MenuBarState) {
    let title = spaced_title(&state.title());
    if state.low() {
        button.setAttributedTitle(&red_title(&title));
    } else {
        button.setAttributedTitle(&NSAttributedString::new());
        button.setTitle(&NSString::from_str(&title));
    }
    button.setImage(Some(&menu_bar_icon::ring_image(
        state.ring_percent(),
        state.low(),
    )));
    button.setAppearsDisabled(state.dimmed());
}

/// The title drawn in the system red, matching the ring in the low state. The
/// colour comes from `NSColor` rather than a literal so it tracks the menu bar
/// appearance and the user's accessibility settings.
fn red_title(title: &str) -> Retained<NSAttributedString> {
    let red = NSColor::systemRedColor();
    // Safety: `NSForegroundColorAttributeName` documents its value as an
    // `NSColor`, which is what we pass.
    unsafe {
        let attrs = NSDictionary::from_slices(
            &[NSForegroundColorAttributeName],
            &[&*red as &objc2::runtime::AnyObject],
        );
        NSAttributedString::new_with_attributes(&NSString::from_str(title), &attrs)
    }
}

/// Height of the primary screen (the one whose origin is 0,0), used as the
/// reference for flipping AppKit's y axis.
fn primary_screen_height(mtm: MainThreadMarker) -> Option<f64> {
    let screens = NSScreen::screens(mtm);
    let mut fallback: Option<f64> = None;
    for screen in screens.iter() {
        let frame = screen.frame();
        if fallback.is_none() {
            fallback = Some(frame.size.height);
        }
        if frame.origin == NSPoint::new(0.0, 0.0) {
            return Some(frame.size.height);
        }
    }
    fallback
}

/// Run as a menu bar accessory: no Dock icon, no menu bar menus, never the
/// active app.
pub fn set_accessory_activation_policy(mtm: MainThreadMarker) {
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
}

impl Drop for StatusItem {
    fn drop(&mut self) {
        if let Some(monitor) = self.outside_monitor.take() {
            unsafe { NSEvent::removeMonitor(&monitor) };
        }
    }
}

/// Watch for mouse-downs in other applications so the popover can dismiss.
fn install_outside_click_monitor(
    tx: UnboundedSender<StatusItemEvent>,
) -> Option<Retained<objc2::runtime::AnyObject>> {
    let mask =
        NSEventMask::LeftMouseDown | NSEventMask::RightMouseDown | NSEventMask::OtherMouseDown;
    let handler = RcBlock::new(move |_event: core::ptr::NonNull<NSEvent>| {
        let _ = tx.unbounded_send(StatusItemEvent::ClickedOutside);
    });
    let monitor = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(mask, &handler);
    if monitor.is_none() {
        eprintln!("claudebar: global mouse monitor unavailable; outside clicks will not dismiss");
    }
    monitor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_percentage_is_spaced_off_the_ring_that_follows_it() {
        assert_eq!(spaced_title("2%"), "2% ");
    }

    #[test]
    fn no_percentage_means_no_title_at_all_not_a_stray_space() {
        assert_eq!(spaced_title(""), "");
    }

    #[test]
    fn the_idle_state_is_a_dimmed_empty_ring_with_no_number() {
        let idle = MenuBarState::Idle;
        assert_eq!(idle.title(), "");
        assert_eq!(idle.ring_percent(), 0.0);
        assert!(!idle.low());
        assert!(idle.dimmed());
    }

    #[test]
    fn a_usage_state_shows_a_whole_percent_and_is_not_dimmed() {
        let state = MenuBarState::Usage {
            percent: 2,
            low: false,
        };
        assert_eq!(state.title(), "2%");
        assert_eq!(state.ring_percent(), 2.0);
        assert!(!state.dimmed());
    }

    #[test]
    fn only_a_low_usage_state_goes_red() {
        assert!(MenuBarState::Usage {
            percent: 92,
            low: true,
        }
        .low());
        assert!(!MenuBarState::Usage {
            percent: 92,
            low: false,
        }
        .low());
        assert!(!MenuBarState::Idle.low());
    }
}
