//! Root popover view: header, notice line, rings, the two local blocks, footer.
//!
//! It owns the little state there is — the theme picked from the window's
//! appearance and whether the "···" menu is open — plus the key bindings and
//! the menu itself. [`rings`] and [`stats`] are pure render functions over the
//! snapshot this view exposes, exactly as `docs/mockup-popover.dc.html` stacks
//! them.
//!
//! Everything it knows about the account and the session logs arrives through
//! [`UsageProvider`], so the whole view tree renders against
//! [`StubProvider`](crate::ui::provider::StubProvider) with no network and no
//! token — see `examples/popover_preview.rs`.

use std::rc::Rc;

use chrono::{DateTime, Local};
use gpui::prelude::FluentBuilder;
use gpui::{
    actions, div, px, App, Context, EventEmitter, FocusHandle, Focusable, FontWeight,
    InteractiveElement, IntoElement, KeyBinding, ParentElement, Pixels, Render, SharedString,
    StatefulInteractiveElement, Styled, Window,
};

use crate::launch_at_login;
use crate::model::{DayStats, LocalStats};
use crate::ui::provider::{ProviderState, UsageProvider};
use crate::ui::theme::{self, Theme};
use crate::ui::{rings, stats};

/// Where the "claude.ai" footer button and menu item go.
pub const USAGE_URL: &str = "https://claude.ai/settings/usage";

/// What the popover asks its window owner to do.
pub enum PopoverEvent {
    /// Esc, or any other dismissal — close the window.
    Close,
    /// Refresh was asked for — the owner refetches and redraws the menu bar
    /// icon, which the popover itself cannot reach.
    Refresh,
}

actions!(claudebar, [Refresh, Dismiss]);

/// Key context for the popover. There is only one screen, so there is only one.
pub const KEY_CONTEXT: &str = "Claudebar";

/// Install the popover's key bindings. Call once at app start.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", Dismiss, Some(KEY_CONTEXT)),
        KeyBinding::new("r", Refresh, Some(KEY_CONTEXT)),
    ]);
}

pub struct Popover {
    focus: FocusHandle,
    provider: Rc<dyn UsageProvider>,
    menu_open: bool,
    /// The last "Launch at login" failure, shown under the menu row.
    login_error: Option<SharedString>,
    theme: Theme,
    appearance: Option<gpui::Subscription>,
}

impl Popover {
    /// A popover over the fixture provider — what the preview example uses, and
    /// what the binary falls back to before the real layers are wired up.
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self::with_provider(Rc::new(crate::ui::provider::StubProvider::new()), cx)
    }

    /// Build a popover over any provider.
    pub fn with_provider(provider: Rc<dyn UsageProvider>, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            provider,
            menu_open: false,
            login_error: None,
            theme: Theme::default(),
            appearance: None,
        }
    }

    // ── What the leaves read ────────────────────────────────────────────────

    pub fn theme(&self) -> Theme {
        self.theme
    }

    pub fn provider(&self) -> &Rc<dyn UsageProvider> {
        &self.provider
    }

    /// The clock the reset captions and the footer read.
    pub fn now(&self) -> DateTime<Local> {
        Local::now()
    }

    /// The seven days of local stats, if a scan has landed.
    pub fn local_stats(&self) -> Option<LocalStats> {
        self.provider.local()
    }

    /// Today's row of the local stats, if a scan has landed.
    pub fn today_stats(&self) -> Option<DayStats> {
        self.provider.local().and_then(|s| s.today().cloned())
    }

    /// Whether there is no token to fetch limits with; the rings are replaced
    /// by one line of instructions, and the local blocks carry on regardless.
    pub fn signed_out(&self) -> bool {
        self.provider.state() == ProviderState::SignedOut
    }

    // ── State changes ───────────────────────────────────────────────────────

    /// Back to the default state; called every time the popover opens.
    pub fn reset(&mut self, cx: &mut Context<Self>) {
        self.menu_open = false;
        self.login_error = None;
        self.reload(cx);
    }

    /// Redraw against whatever the provider holds now. The provider owns the
    /// snapshots, so there is nothing to copy — this is the hook the owner
    /// calls when a fetch lands.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }

    /// Ask for a refetch and tell the owner, which also redraws the menu bar.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.menu_open = false;
        self.provider.refresh();
        cx.emit(PopoverEvent::Refresh);
        self.reload(cx);
    }

    fn toggle_menu(&mut self, cx: &mut Context<Self>) {
        self.menu_open = !self.menu_open;
        cx.notify();
    }

    fn open_usage_page(&mut self, cx: &mut Context<Self>) {
        self.menu_open = false;
        cx.open_url(USAGE_URL);
    }

    // ── Actions ─────────────────────────────────────────────────────────────

    fn on_dismiss(&mut self, _: &Dismiss, _: &mut Window, cx: &mut Context<Self>) {
        // Esc collapses the menu first, so one press never does two things.
        if self.menu_open {
            self.menu_open = false;
            cx.notify();
        } else {
            cx.emit(PopoverEvent::Close);
        }
    }

    // ── Layout ──────────────────────────────────────────────────────────────

    /// Height the content wants; the window is resized to it.
    pub fn preferred_height(&self) -> Pixels {
        let limits = if self.signed_out() {
            SIGNED_OUT_HEIGHT
        } else {
            rings::height()
        };
        let notice = if self.notice().is_some() {
            NOTICE_HEIGHT
        } else {
            0.0
        };
        px(HEADER_HEIGHT
            + notice
            + limits
            + RULE
            + stats::today_height(self)
            + RULE
            + stats::week_height()
            + RULE
            + FOOTER_HEIGHT)
    }

    /// The muted line under the header, when there is something to say. A
    /// healthy popover has nothing there: the rings already say it.
    fn notice(&self) -> Option<String> {
        match self.provider.state() {
            ProviderState::Error(message) => Some(message),
            ProviderState::SignedOut => Some("Not signed in".into()),
            ProviderState::Loading | ProviderState::Ready => None,
        }
    }

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let plan = self.provider.usage().and_then(|u| u.plan);

        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px(theme::PAD_X)
            .pt(px(HEADER_PAD_TOP))
            .pb(px(HEADER_PAD_BOTTOM))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_baseline()
                    .gap(px(6.))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(theme::TEXT_TITLE)
                            .child("Claude"),
                    )
                    .when_some(plan, |el, plan| {
                        el.child(
                            div()
                                .text_size(theme::TEXT_SMALL)
                                .text_color(theme.secondary)
                                .child(plan),
                        )
                    }),
            )
            .child(icon_button(
                "more",
                "···",
                theme,
                cx.listener(|this, _, _, cx| this.toggle_menu(cx)),
            ))
    }

    fn notice_line(&self) -> Option<impl IntoElement> {
        let theme = self.theme;
        self.notice().map(|line| {
            div()
                .h(px(NOTICE_HEIGHT))
                .px(theme::PAD_X)
                .text_size(theme::TEXT_TINY)
                .text_color(theme.secondary)
                .child(line)
        })
    }

    /// What stands in for the rings with no token: the one thing the user can
    /// do about it.
    fn signed_out_line(&self) -> impl IntoElement {
        let theme = self.theme;
        div()
            .h(px(SIGNED_OUT_HEIGHT))
            .flex()
            .items_center()
            .justify_center()
            .px(theme::PAD_X)
            .text_size(theme::TEXT_SMALL)
            .text_color(theme.secondary)
            .child("Sign in with `claude` in a terminal first")
    }

    /// The "Launch at login" row: a checkmark that reflects `SMAppService`'s
    /// own status, so it agrees with System Settings rather than with a local
    /// copy of the state. Disabled outside a bundle, with the reason under it.
    fn launch_at_login_item(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let available = launch_at_login::availability() == launch_at_login::Availability::Ready;
        let checked = launch_at_login::is_enabled();
        let note = |message: SharedString| {
            div()
                .px(px(10.))
                .pb(px(4.))
                .text_size(theme::TEXT_MICRO)
                .line_height(px(13.))
                .text_color(theme.tertiary)
                .child(message)
        };

        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("menu-login")
                    .px(px(10.))
                    .py(px(5.))
                    .rounded(px(5.))
                    .text_size(theme::TEXT_SMALL)
                    .line_height(px(17.))
                    .text_color(if available {
                        theme.text
                    } else {
                        theme.tertiary
                    })
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .when(available, |el| {
                        el.cursor_pointer()
                            .hover(|s| s.bg(theme.hover))
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_launch_at_login(cx)))
                    })
                    .child("Launch at login")
                    .child(if checked { "\u{2713}" } else { "" }),
            )
            .when(!available, |el| {
                el.child(note(launch_at_login::NO_BUNDLE_NOTE.into()))
            })
            .when_some(self.login_error.clone(), |el, message| {
                el.child(note(message))
            })
    }

    /// Flip the login item, keeping whatever `SMAppService` complained about so
    /// the menu can show it rather than failing silently.
    fn toggle_launch_at_login(&mut self, cx: &mut Context<Self>) {
        let wanted = !launch_at_login::is_enabled();
        self.login_error = launch_at_login::set_enabled(wanted).err().map(Into::into);
        if self.login_error.is_none() {
            self.menu_open = false;
        }
        cx.notify();
    }

    /// The "···" dropdown, drawn over the content.
    fn menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let item = |id: &'static str,
                    label: &'static str,
                    cx: &mut Context<Self>,
                    action: fn(&mut Self, &mut Context<Self>)| {
            div()
                .id(id)
                .px(px(10.))
                .py(px(5.))
                .rounded(px(5.))
                .text_size(theme::TEXT_SMALL)
                .text_color(theme.text)
                .cursor_pointer()
                .hover(|s| s.bg(theme.hover))
                .on_click(cx.listener(move |this, _, _, cx| action(this, cx)))
                .child(label)
        };

        div()
            .absolute()
            // Without this the content underneath gets the same click.
            .occlude()
            .top(px(HEADER_HEIGHT - 2.0))
            .right(theme::PAD_X)
            .w(px(160.))
            .p(px(4.))
            .rounded(px(8.))
            .bg(theme.menu_bg)
            .border_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .child(item("menu-refresh", "Refresh", cx, |this, cx| {
                this.refresh(cx)
            }))
            .child(self.launch_at_login_item(cx))
            .child(div().h(px(RULE)).my(px(4.)).bg(theme.separator))
            .child(item("menu-quit", "Quit Claudebar", cx, |this, cx| {
                this.menu_open = false;
                cx.quit();
            }))
    }

    fn rule(&self) -> impl IntoElement {
        div().h(px(RULE)).mx(theme::PAD_X).bg(self.theme.separator)
    }

    fn footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let text_button = |id: &'static str,
                           label: &'static str,
                           cx: &mut Context<Self>,
                           action: fn(&mut Self, &mut Context<Self>)| {
            div()
                .id(id)
                .cursor_pointer()
                .text_color(theme.accent)
                .hover(|s| s.text_color(theme.text))
                .on_click(cx.listener(move |this, _, _, cx| action(this, cx)))
                .child(label)
        };

        let left = match self.provider.updated_at() {
            Some(at) => format!("Updated {}", at.format("%-I:%M %p")),
            None => "Not updated yet".to_string(),
        };

        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px(theme::PAD_X)
            .pt(px(FOOTER_PAD_TOP))
            .pb(px(FOOTER_PAD_BOTTOM))
            .text_size(theme::TEXT_TINY)
            .text_color(theme.tertiary)
            .child(div().child(left))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(12.))
                    .child(text_button("footer-refresh", "Refresh", cx, |this, cx| {
                        this.refresh(cx)
                    }))
                    .child(text_button("footer-web", "claude.ai", cx, |this, cx| {
                        this.open_usage_page(cx)
                    })),
            )
    }
}

/// A 22px square glyph button: the "···" menu.
fn icon_button(
    id: &'static str,
    glyph: &'static str,
    theme: Theme,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .size(theme::ICON_BUTTON)
        .flex_shrink_0()
        .rounded(px(5.))
        .text_size(theme::TEXT_BODY)
        .text_color(theme.secondary)
        .cursor_pointer()
        .hover(|s| s.bg(theme.hover).text_color(theme.text))
        .on_click(on_click)
        .child(glyph)
}

// ── Layout constants (see `docs/mockup-popover.dc.html`) ─────────────────────

/// The mockup's `12px 14px 4px` header padding around a 22px button row.
const HEADER_PAD_TOP: f32 = 12.0;
const HEADER_PAD_BOTTOM: f32 = 4.0;
pub const HEADER_HEIGHT: f32 = HEADER_PAD_TOP + 22.0 + HEADER_PAD_BOTTOM;
/// The muted status line under the header.
pub const NOTICE_HEIGHT: f32 = 18.0;
/// The line that replaces the rings with no token.
pub const SIGNED_OUT_HEIGHT: f32 = 10.0 + 19.0 + 14.0;
/// A hairline separator.
pub const RULE: f32 = 1.0;
/// The mockup's `8px 14px 9px` footer padding around an 11px line.
const FOOTER_PAD_TOP: f32 = 8.0;
const FOOTER_PAD_BOTTOM: f32 = 9.0;
pub const FOOTER_HEIGHT: f32 = FOOTER_PAD_TOP + 18.0 + FOOTER_PAD_BOTTOM;

impl EventEmitter<PopoverEvent> for Popover {}

impl Focusable for Popover {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Popover {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Follow the system appearance without the views having to ask: the
        // window only repaints when something notifies it, so a light/dark flip
        // while the popover is up has to wake it explicitly.
        if self.appearance.is_none() {
            let this = cx.entity();
            self.appearance = Some(window.observe_window_appearance(move |_window, cx| {
                this.update(cx, |_, cx| cx.notify());
            }));
        }
        self.theme = Theme::for_appearance(window.appearance());
        let theme = self.theme;

        let header = self.header(cx);
        let notice = self.notice_line();
        let limits: gpui::AnyElement = if self.signed_out() {
            self.signed_out_line().into_any_element()
        } else {
            rings::render(self, cx).into_any_element()
        };
        let today = stats::today(self, cx);
        let week = stats::week(self, cx);
        let footer = self.footer(cx);
        // A transparent backdrop so a click anywhere dismisses the menu instead
        // of falling through to a button.
        let backdrop = self.menu_open.then(|| {
            div()
                .id("menu-backdrop")
                .absolute()
                .inset_0()
                .occlude()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.menu_open = false;
                    cx.notify();
                }))
        });
        let menu = self.menu_open.then(|| self.menu(cx));

        div()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Refresh, _, cx| this.refresh(cx)))
            .on_action(cx.listener(Self::on_dismiss))
            .relative()
            .flex()
            .flex_col()
            .w(theme::POPOVER_WIDTH)
            .h_full()
            .bg(theme.bg)
            .rounded(theme::POPOVER_RADIUS)
            .border_1()
            .border_color(theme.border)
            .overflow_hidden()
            .font_family(theme::UI_FAMILY)
            .text_size(theme::TEXT_BODY)
            .text_color(theme.text)
            .child(header)
            .children(notice)
            .child(limits)
            .child(self.rule())
            .child(today)
            .child(self.rule())
            .child(week)
            .child(self.rule())
            .child(footer)
            .children(backdrop)
            .children(menu)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The heights the window is sized from, exercised without a gpui window:
    /// they are plain arithmetic over the section constants.
    fn total(limits: f32, notice: f32, today: f32) -> f32 {
        HEADER_HEIGHT
            + notice
            + limits
            + RULE
            + today
            + RULE
            + stats::week_height()
            + RULE
            + FOOTER_HEIGHT
    }

    #[test]
    fn the_ready_popover_is_the_mockups_height() {
        // Header 38, rings 129, Today with two model rows 171, week 99,
        // footer 35, three rules.
        assert_eq!(total(rings::height(), 0.0, 171.0), 475.0);
    }

    #[test]
    fn a_notice_and_the_signed_out_line_change_the_total() {
        let ready = total(rings::height(), 0.0, 171.0);
        assert_eq!(total(rings::height(), NOTICE_HEIGHT, 171.0) - ready, 18.0);
        assert!(total(SIGNED_OUT_HEIGHT, NOTICE_HEIGHT, 171.0) < ready);
    }

    #[test]
    fn the_usage_page_is_the_settings_tab() {
        assert_eq!(USAGE_URL, "https://claude.ai/settings/usage");
    }
}
