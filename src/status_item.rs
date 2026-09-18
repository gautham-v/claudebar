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
    NSAppearanceCustomization, NSApplication, NSApplicationActivationPolicy, NSCellImagePosition,
    NSControl, NSEvent, NSEventMask, NSScreen, NSStatusBar, NSStatusBarButton, NSStatusItem,
    NSVariableStatusItemLength, NSView,
};
use objc2_foundation::{NSPoint, NSRect};

use crate::menu_bar_icon::{self, ItemPart, MenuBarInk};

/// What the menu bar item is showing.
#[derive(Debug, Clone, PartialEq)]
pub enum MenuBarState {
    /// Loading, signed out, or otherwise nothing to report: an empty ring, no
    /// number, and AppKit's own disabled rendering.
    Idle,
    /// One part per limit the user picked, in the order they are drawn, plus
    /// the two choices that apply to the whole item: whether it prints the
    /// numbers and whether it draws the rings.
    Usage {
        parts: Vec<ItemPart>,
        show_percent: bool,
        show_rings: bool,
    },
}

impl MenuBarState {
    /// Idle states are faded, the way the system items fade when they have
    /// nothing to say. A ring-only item is not idle: it has something to
    /// report, it just reports it without a number, so it stays full strength.
    fn dimmed(&self) -> bool {
        matches!(self, MenuBarState::Idle)
    }

    /// What [`menu_bar_icon::item_image`] should draw: the parts and the two
    /// flags. Idle is one empty ring and no number.
    fn drawing(&self) -> (Vec<ItemPart>, bool, bool) {
        match self {
            MenuBarState::Idle => (
                vec![ItemPart {
                    tag: None,
                    percent: 0.0,
                    low: false,
                }],
                false,
                true,
            ),
            MenuBarState::Usage {
                parts,
                show_percent,
                show_rings,
            } => (parts.clone(), *show_percent, *show_rings),
        }
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

/// Where the menu bar item is: its own frame and the frame of the display it
/// is currently on, both in gpui's screen coordinate space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchor {
    pub item: ScreenRect,
    pub screen: ScreenRect,
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
    /// The item is one image of every picked limit — percentage then ring, in
    /// the menu bar's own font — so it sits like the battery item. Callers
    /// pass the state they have; [`MenuBarState::Idle`] leaves one empty ring
    /// with no number, and it stays clickable because the image has a hit area
    /// of its own.
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
                // The whole item — numbers, tags and rings — is the image, so
                // the button has no title to place beside it.
                button.setImagePosition(NSCellImagePosition::ImageOnly);
                let control: &NSControl = &button;
                control.setTarget(Some(&*target));
                control.setAction(Some(sel!(claudebarStatusItemClicked:)));
            }
            apply_state(&button, &state);
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
            apply_state(&button, &state);
        }
    }

    /// Where the item is and which display it is on, in gpui screen
    /// coordinates.
    ///
    /// AppKit hands back bottom-left-origin rects on the status bar's own
    /// window; gpui wants top-left-origin relative to the primary display, so
    /// we flip both through the primary screen's height.
    ///
    /// The screen comes along because macOS moves the menu bar — and every
    /// status item with it — to whichever display has the user's attention,
    /// and that display is often not the primary one. gpui reports every
    /// display as if it began at the origin, so the popover has to be clamped
    /// against this rectangle; clamping against the primary display's size
    /// pins the popover to the primary display's right edge whenever the item
    /// is sitting further right than that display is wide.
    pub fn anchor(&self, mtm: MainThreadMarker) -> Option<Anchor> {
        let button = self.item.button(mtm)?;
        let window = button.window()?;
        let frame: NSRect = window.frame();
        let flip_height = primary_screen_height(mtm)?;
        let screen = screen_containing(mtm, frame)?;

        Some(Anchor {
            item: flipped(frame, flip_height),
            screen: flipped(screen, flip_height),
        })
    }
}

/// An AppKit rect in gpui's screen coordinates.
fn flipped(frame: NSRect, flip_height: f64) -> ScreenRect {
    ScreenRect {
        x: frame.origin.x as f32,
        // Top edge in flipped coords.
        y: (flip_height - (frame.origin.y + frame.size.height)) as f32,
        width: frame.size.width as f32,
        height: frame.size.height as f32,
    }
}

/// The frame of the screen `rect` sits on, by its midpoint — `NSWindow`'s own
/// `screen` is nil for a window AppKit considers offscreen, and the status
/// item's window is an odd enough one not to trust that on. Falls back to the
/// primary screen, which is where the menu bar is unless a second display has
/// the user's attention.
fn screen_containing(mtm: MainThreadMarker, rect: NSRect) -> Option<NSRect> {
    let mid = NSPoint::new(
        rect.origin.x + rect.size.width / 2.0,
        rect.origin.y + rect.size.height / 2.0,
    );
    let screens = NSScreen::screens(mtm);
    let mut primary: Option<NSRect> = None;
    for screen in screens.iter() {
        let frame = screen.frame();
        if primary.is_none() || frame.origin == NSPoint::new(0.0, 0.0) {
            primary = Some(frame);
        }
        let inside = mid.x >= frame.origin.x
            && mid.x <= frame.origin.x + frame.size.width
            && mid.y >= frame.origin.y
            && mid.y <= frame.origin.y + frame.size.height;
        if inside {
            return Some(frame);
        }
    }
    primary
}

/// Push a state onto the button: the picture and the fade.
///
/// Everything visible is one image (see [`menu_bar_icon::item_image`]): a
/// status item button draws one title and one image, and two limits need two
/// rings, so the numbers are drawn into the image rather than set as a title.
/// That also settles an old annoyance — a plain title picks up AppKit's 13pt
/// control font, which sat visibly larger than the battery percentage next
/// door, and the font is now spelled out in one place.
fn apply_state(button: &NSStatusBarButton, state: &MenuBarState) {
    let (parts, show_percent, show_rings) = state.drawing();
    button.setImage(Some(&menu_bar_icon::item_image(
        &parts,
        show_percent,
        show_rings,
        menu_bar_ink(button),
    )));
    button.setAppearsDisabled(state.dimmed());
}

/// Which ink the menu bar is drawing in, read off the status item button
/// itself.
///
/// Only an item that has gone red needs this — everything else is a template
/// image the menu bar tints itself — and the button's own appearance is the
/// only honest source: the menu bar over a pale wallpaper is light even in
/// dark mode. The names are `NSAppearanceNameVibrantDark`, `...DarkAqua` and
/// the high-contrast variants of both, so "Dark" anywhere in the name is the
/// test. Read when the image is built, which means a theme change mid-window
/// is picked up by the next fetch rather than the instant it happens.
fn menu_bar_ink(button: &NSStatusBarButton) -> MenuBarInk {
    let view: &NSView = button;
    if view
        .effectiveAppearance()
        .name()
        .to_string()
        .contains("Dark")
    {
        MenuBarInk::White
    } else {
        MenuBarInk::Black
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

    fn part(percent: f32, low: bool) -> ItemPart {
        ItemPart {
            tag: None,
            percent,
            low,
        }
    }

    /// Idle draws one empty ring and no number, and takes AppKit's fade.
    #[test]
    fn the_idle_state_is_a_dimmed_empty_ring_with_no_number() {
        let (parts, show_percent, show_rings) = MenuBarState::Idle.drawing();
        assert_eq!(parts, vec![part(0.0, false)]);
        assert!(!show_percent);
        assert!(show_rings);
        assert!(MenuBarState::Idle.dimmed());
    }

    /// A state with numbers is never faded — it has something to say.
    #[test]
    fn a_usage_state_draws_what_it_was_given_and_is_not_dimmed() {
        let state = MenuBarState::Usage {
            parts: vec![
                ItemPart {
                    tag: Some("5h".into()),
                    percent: 5.0,
                    low: false,
                },
                ItemPart {
                    tag: Some("wk".into()),
                    percent: 92.0,
                    low: true,
                },
            ],
            show_percent: true,
            show_rings: true,
        };
        let (parts, show_percent, show_rings) = state.drawing();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1].tag.as_deref(), Some("wk"));
        assert!(parts[1].low);
        assert!(show_percent);
        assert!(show_rings);
        assert!(!state.dimmed());
    }

    /// The ring-only and number-only choices reach the drawing code as they
    /// were set; `item_image` is what refuses to draw nothing at all.
    #[test]
    fn the_two_drawing_choices_are_carried_through() {
        let state = MenuBarState::Usage {
            parts: vec![part(42.0, false)],
            show_percent: false,
            show_rings: true,
        };
        assert!(!state.drawing().1);
        let state = MenuBarState::Usage {
            parts: vec![part(42.0, false)],
            show_percent: true,
            show_rings: false,
        };
        assert!(!state.drawing().2);
    }
}
