//! Root popover view: the header, a block per limit, the two local blocks and
//! the menu rows under them.
//!
//! It is shaped like a system menu — the Battery item's menu, which
//! `docs/mockup-popover-v2.dc.html` copies: 260px wide, 5px of inset around
//! plain rows, bold section headers, hairline separators, and nothing but ink
//! on the material except a limit bar that has gone red.
//!
//! It owns the little state there is — the theme picked from the window's
//! appearance and whether the Settings row is expanded — plus the key bindings.
//! [`stats`] renders the two blocks read out of the local session logs.
//!
//! Everything it knows about the account and the session logs arrives through
//! [`UsageProvider`], so the whole view tree renders against
//! [`StubProvider`](crate::ui::provider::StubProvider) with no network and no
//! token — see `examples/popover_preview.rs`.

use std::rc::Rc;

use chrono::{DateTime, Local};
use gpui::prelude::FluentBuilder;
use gpui::{
    actions, div, px, App, Context, Div, EventEmitter, FocusHandle, Focusable, FontWeight,
    InteractiveElement, IntoElement, KeyBinding, ParentElement, Pixels, Render, SharedString,
    StatefulInteractiveElement, Styled, Window,
};

use crate::launch_at_login;
use crate::model::{DayStats, Limit, LimitKind, LocalStats};
use crate::settings::{MenuBarLimit, Settings};
use crate::ui::provider::{ProviderState, UsageProvider};
use crate::ui::stats;
use crate::ui::theme::{self, Theme};

/// Where the "Open claude.ai" row goes.
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
    /// Whether the "Settings" disclosure row has its rows shown under it.
    settings_open: bool,
    /// The last "Launch at login" failure, shown under that row.
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
            settings_open: false,
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

    /// The clock the reset subtitles read.
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

    /// Whether there is no token to fetch limits with; the limit blocks are
    /// replaced by one line of instructions, and the local blocks carry on
    /// regardless.
    pub fn signed_out(&self) -> bool {
        self.provider.state() == ProviderState::SignedOut
    }

    // ── State changes ───────────────────────────────────────────────────────

    /// Back to the default state; called every time the popover opens.
    pub fn reset(&mut self, cx: &mut Context<Self>) {
        self.settings_open = false;
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
        self.settings_open = false;
        self.provider.refresh();
        cx.emit(PopoverEvent::Refresh);
        self.reload(cx);
    }

    /// Expand the Settings section without a click — the preview example's
    /// `settings` mode, which cannot click.
    pub fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.settings_open = true;
        cx.notify();
    }

    fn toggle_settings(&mut self, cx: &mut Context<Self>) {
        self.settings_open = !self.settings_open;
        cx.notify();
    }

    fn open_usage_page(&mut self, cx: &mut Context<Self>) {
        self.settings_open = false;
        cx.open_url(USAGE_URL);
    }

    // ── Actions ─────────────────────────────────────────────────────────────

    fn on_dismiss(&mut self, _: &Dismiss, _: &mut Window, cx: &mut Context<Self>) {
        // Esc collapses the Settings section first, so one press never does
        // two things.
        if self.settings_open {
            self.settings_open = false;
            cx.notify();
        } else {
            cx.emit(PopoverEvent::Close);
        }
    }

    // ── Layout ──────────────────────────────────────────────────────────────

    /// Height the content wants; the window is resized to it.
    pub fn preferred_height(&self) -> Pixels {
        let notice = if self.notice().is_some() {
            NOTICE_HEIGHT
        } else {
            0.0
        };
        px(POPOVER_PAD_TOTAL
            + SECTION_HEADER_HEIGHT
            + notice
            + self.limits_height()
            + SEPARATOR_HEIGHT
            + stats::today_height()
            + SEPARATOR_HEIGHT
            + stats::week_height()
            + SEPARATOR_HEIGHT
            + self.menu_height())
    }

    /// How tall the block under the header is: one block per limit, or the one
    /// line that stands in for them when there is nothing to show.
    fn limits_height(&self) -> f32 {
        let limits = self.limits();
        if limits.is_empty() {
            return LIMITS_PAD_TOP + theme::LINE_SMALL_PX + LIMITS_PAD_BOTTOM;
        }
        let n = limits.len() as f32;
        LIMITS_PAD_TOP
            + n * LIMIT_BLOCK_HEIGHT
            + (n - 1.0) * theme::LIMIT_BLOCK_GAP_PX
            + LIMITS_PAD_BOTTOM
    }

    /// How tall the menu rows at the bottom are, from the rows they will draw.
    /// Kept as arithmetic over the row constants rather than measured, because
    /// `preferred_height` runs before the rows are laid out.
    fn menu_height(&self) -> f32 {
        // Refresh, Open claude.ai, Settings, Launch at login, then Quit under
        // its own separator.
        let rows = 5.0;
        let settings = if self.settings_open {
            let rows = self.menu_bar_choices().len() + self.drawing_rows().len();
            SECTION_LABEL_HEIGHT + rows as f32 * ROW_HEIGHT
        } else {
            0.0
        };
        let login_note = if launch_at_login::availability() != launch_at_login::Availability::Ready
        {
            NOTE_HEIGHT
        } else {
            0.0
        };
        let login_error = if self.login_error.is_some() {
            NOTE_HEIGHT
        } else {
            0.0
        };
        rows * ROW_HEIGHT + settings + SEPARATOR_HEIGHT + login_note + login_error
    }

    /// The muted line under the header, when there is something to say. A
    /// healthy popover has nothing there: the limit blocks already say it.
    /// Signed out is not a notice — it replaces the blocks outright.
    fn notice(&self) -> Option<String> {
        match self.provider.state() {
            ProviderState::Error(message) => Some(message),
            ProviderState::SignedOut | ProviderState::Loading | ProviderState::Ready => None,
        }
    }

    /// The limits to draw, which is nothing at all until a fetch lands.
    fn limits(&self) -> Vec<Limit> {
        if self.signed_out() {
            return Vec::new();
        }
        self.provider.usage().map(|u| u.limits).unwrap_or_default()
    }

    fn notice_line(&self) -> Option<impl IntoElement> {
        let theme = self.theme;
        self.notice().map(|line| {
            div()
                .px(theme::ROW_PAD_X)
                .h(px(NOTICE_HEIGHT))
                .text_size(theme::TEXT_TINY)
                .line_height(theme::LINE_TINY)
                .text_color(theme.secondary)
                .child(line)
        })
    }

    /// One limit: its name and percent on a row, a 4px bar, and when the
    /// window resets under both.
    fn limit_block(&self, limit: &Limit) -> impl IntoElement {
        let theme = self.theme;
        let threshold = self.provider.settings().low_remaining_percent;
        let percent = limit.percent_rounded();
        let caption = limit.reset_sentence(self.now()).unwrap_or_default();

        div()
            .flex()
            .flex_col()
            .gap(theme::LIMIT_GAP)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .text_size(theme::TEXT_BODY)
                    .line_height(theme::LINE_TITLE)
                    .child(div().child(limit.label().to_string()))
                    .child(
                        div()
                            .text_color(theme.secondary)
                            .child(format!("{percent}%")),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .h(theme::LIMIT_BAR_HEIGHT)
                    .rounded(theme::LIMIT_BAR_RADIUS)
                    .bg(theme.separator)
                    .overflow_hidden()
                    .child(
                        div()
                            .w(gpui::relative(percent as f32 / 100.0))
                            .h_full()
                            .rounded(theme::LIMIT_BAR_RADIUS)
                            .bg(theme.limit_bar(limit.is_low(threshold))),
                    ),
            )
            .child(
                div()
                    .h(theme::LINE_TINY)
                    .text_size(theme::TEXT_TINY)
                    .line_height(theme::LINE_TINY)
                    .text_color(theme.secondary)
                    .child(caption),
            )
    }

    /// The block under the header: one entry per limit, or the single line
    /// that explains why there are none.
    fn limits_block(&self) -> impl IntoElement {
        let theme = self.theme;
        let limits = self.limits();
        let frame = div()
            .flex()
            .flex_col()
            .gap(theme::LIMIT_BLOCK_GAP)
            .px(theme::ROW_PAD_X)
            .pt(px(LIMITS_PAD_TOP))
            .pb(px(LIMITS_PAD_BOTTOM));

        if limits.is_empty() {
            // Under an error the notice line already says what happened;
            // "checking" would be untrue.
            let line = if self.signed_out() {
                SIGNED_OUT_LINE
            } else if matches!(self.provider.state(), ProviderState::Error(_)) {
                NO_NUMBERS_LINE
            } else {
                LOADING_LINE
            };
            return frame.child(
                div()
                    .h(theme::LINE_SMALL)
                    .text_size(theme::TEXT_SMALL)
                    .line_height(theme::LINE_SMALL)
                    .text_color(theme.secondary)
                    .child(line),
            );
        }
        frame.children(
            limits
                .iter()
                .map(|limit| self.limit_block(limit).into_any_element())
                .collect::<Vec<_>>(),
        )
    }

    /// A plain menu row: a label with a hover wash, like a menu item.
    fn menu_row(
        &self,
        id: SharedString,
        label: SharedString,
        indented: bool,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> Div {
        self.check_row(id, label, None, indented, true, cx, action)
    }

    /// A menu row, with an optional checkmark on the right. Every clickable
    /// row in the popover goes through this so they all wash the same way.
    #[allow(clippy::too_many_arguments)]
    fn check_row(
        &self,
        id: SharedString,
        label: SharedString,
        checked: Option<bool>,
        indented: bool,
        enabled: bool,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> Div {
        let theme = self.theme;
        let row = row_frame()
            .id(id)
            .justify_between()
            .items_center()
            .when(indented, |el| el.pl(theme::ROW_PAD_X + theme::ROW_INDENT))
            .rounded(theme::ROW_RADIUS)
            .text_size(theme::TEXT_BODY)
            .line_height(theme::LINE_TITLE)
            .text_color(if enabled { theme.text } else { theme.tertiary })
            .when(enabled, |el| {
                el.cursor_pointer()
                    .hover(|s| s.bg(theme.hover))
                    .on_click(cx.listener(move |this, _, _, cx| action(this, cx)))
            })
            .child(label)
            .children(checked.map(|on| div().child(if on { "\u{2713}" } else { "" })));
        // The rows live in a flex column, and gpui's `Stateful<Div>` is not a
        // `Div`; wrapping keeps every child of the column the same type.
        div().flex().flex_col().child(row)
    }

    /// A note under a row (no bundle, or a failed toggle).
    fn note(&self, message: SharedString) -> impl IntoElement {
        div()
            .px(theme::ROW_PAD_X)
            .pb(px(NOTE_PAD_BOTTOM))
            .text_size(theme::TEXT_MICRO)
            .line_height(theme::LINE_MICRO)
            .text_color(self.theme.tertiary)
            .child(message)
    }

    /// The "Launch at login" row: a checkmark that reflects `SMAppService`'s
    /// own status, so it agrees with System Settings rather than with a local
    /// copy of the state. Disabled outside a bundle, with the reason under it.
    fn launch_at_login_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let available = launch_at_login::availability() == launch_at_login::Availability::Ready;
        self.check_row(
            "row-login".into(),
            "Launch at login".into(),
            Some(launch_at_login::is_enabled()),
            false,
            available,
            cx,
            |this, cx| this.toggle_launch_at_login(cx),
        )
        .when(!available, |el| {
            el.child(self.note(launch_at_login::NO_BUNDLE_NOTE.into()))
        })
        .when_some(self.login_error.clone(), |el, message| {
            el.child(self.note(message))
        })
    }

    /// Flip the login item, keeping whatever `SMAppService` complained about so
    /// the popover can show it rather than failing silently.
    fn toggle_launch_at_login(&mut self, cx: &mut Context<Self>) {
        let wanted = !launch_at_login::is_enabled();
        self.login_error = launch_at_login::set_enabled(wanted).err().map(Into::into);
        cx.notify();
    }

    /// The rows the "Menu bar shows" section offers: one per limit the last
    /// snapshot carried, in the order the blocks are drawn. Before the first
    /// fetch lands there is nothing to enumerate, so the two windows every
    /// account has stand in — picking one of them is still meaningful.
    fn menu_bar_choices(&self) -> Vec<(MenuBarLimit, SharedString)> {
        let limits = self.provider.usage().map(|u| u.limits).unwrap_or_default();
        if limits.is_empty() {
            return vec![
                (MenuBarLimit::Session, "Session".into()),
                (MenuBarLimit::Weekly, "Week".into()),
            ];
        }
        limits
            .iter()
            .map(|limit| {
                let choice = match &limit.kind {
                    LimitKind::Session => MenuBarLimit::Session,
                    LimitKind::Weekly => MenuBarLimit::Weekly,
                    LimitKind::Model(name) => MenuBarLimit::Model(name.clone()),
                };
                (choice, SharedString::from(limit.label().to_string()))
            })
            .collect()
    }

    /// The rows the Settings disclosure shows in place: which limits the menu
    /// bar draws, and how it draws them.
    fn settings_rows(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = self.provider.settings();
        let choices = self.menu_bar_choices();
        // The item has to keep drawing something, so the last checked limit
        // is shown as a checkmark that cannot be cleared rather than one that
        // silently refuses the click.
        let picked = choices
            .iter()
            .filter(|(choice, _)| settings.shows(choice))
            .count();
        div()
            .flex()
            .flex_col()
            .child(section_label(self.theme, MENU_BAR_SECTION))
            .children(
                choices
                    .into_iter()
                    .map(|(choice, label)| {
                        let checked = settings.shows(&choice);
                        self.check_row(
                            SharedString::from(format!("row-limit-{}", choice.as_config_str())),
                            label,
                            Some(checked),
                            true,
                            !(checked && picked == 1),
                            cx,
                            move |this, cx| this.toggle_menu_bar_limit(choice.clone(), cx),
                        )
                        .into_any_element()
                    })
                    .collect::<Vec<_>>(),
            )
            .children(
                self.drawing_rows()
                    .into_iter()
                    .map(|row| {
                        self.check_row(
                            SharedString::from(format!("row-{}", row.id)),
                            row.label.into(),
                            Some(row.checked),
                            true,
                            row.enabled,
                            cx,
                            move |this, cx| this.toggle_drawing(row.id, cx),
                        )
                        .into_any_element()
                    })
                    .collect::<Vec<_>>(),
            )
    }

    /// The rows under the limit list: what the item draws for each limit.
    ///
    /// The labels row only appears when more than one limit is checked — with
    /// one number there is nothing to tell apart, so the question would have
    /// no answer worth giving. The other two cannot both be turned off, or the
    /// item would have no ink at all, so whichever is the last one standing is
    /// shown checked and disabled.
    fn drawing_rows(&self) -> Vec<DrawingRow> {
        let settings = self.provider.settings();
        let picked = self
            .menu_bar_choices()
            .iter()
            .filter(|(choice, _)| settings.shows(choice))
            .count();
        let mut rows = vec![DrawingRow {
            id: "show-percent",
            label: "Show percentage",
            checked: settings.show_percent,
            enabled: settings.show_rings || !settings.show_percent,
        }];
        if picked > 1 {
            rows.push(DrawingRow {
                id: "show-labels",
                label: "Show labels",
                checked: settings.show_labels,
                enabled: true,
            });
        }
        rows.push(DrawingRow {
            id: "show-rings",
            label: "Show rings",
            checked: settings.show_rings,
            enabled: settings.show_percent || !settings.show_rings,
        });
        rows
    }

    /// Add a limit to what the menu bar draws, or take it away again.
    fn toggle_menu_bar_limit(&mut self, limit: MenuBarLimit, cx: &mut Context<Self>) {
        self.update_settings(cx, |settings| {
            if let Some(at) = settings.menu_bar.iter().position(|l| *l == limit) {
                settings.menu_bar.remove(at);
            } else {
                settings.menu_bar.push(limit);
            }
        });
    }

    /// Flip one of the drawing choices, persist it, and collapse.
    fn toggle_drawing(&mut self, id: &'static str, cx: &mut Context<Self>) {
        self.update_settings(cx, |settings| match id {
            "show-percent" => settings.show_percent = !settings.show_percent,
            "show-labels" => settings.show_labels = !settings.show_labels,
            _ => settings.show_rings = !settings.show_rings,
        });
    }

    /// The one path a settings row takes to change a setting: read what the
    /// provider holds, change the one field, hand it back. The provider
    /// persists it and re-renders both the popover and the menu bar item, so
    /// there is nothing to copy into this view.
    fn update_settings(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut Settings)) {
        let mut settings = self.provider.settings();
        change(&mut settings);
        // Every row that could empty the item is disabled before it is
        // clicked, so this is the belt on a hand-edited file's braces rather
        // than a path the menu can take.
        if settings.menu_bar.is_empty() || (!settings.show_percent && !settings.show_rings) {
            return;
        }
        self.provider.set_settings(settings);
        self.settings_open = false;
        cx.notify();
    }

    /// The rows under the last separator: the actions, the Settings
    /// disclosure, and Quit under a separator of its own.
    fn menu_rows(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .child(self.menu_row(
                "row-refresh".into(),
                "Refresh".into(),
                false,
                cx,
                |this, cx| this.refresh(cx),
            ))
            .child(self.menu_row(
                "row-web".into(),
                "Open claude.ai".into(),
                false,
                cx,
                |this, cx| this.open_usage_page(cx),
            ))
            .child(self.menu_row(
                "row-settings".into(),
                "Settings".into(),
                false,
                cx,
                |this, cx| this.toggle_settings(cx),
            ))
            .children(self.settings_open.then(|| self.settings_rows(cx)))
            .child(self.launch_at_login_row(cx))
            .child(separator(self.theme))
            .child(self.menu_row(
                "row-quit".into(),
                "Quit Claudebar".into(),
                false,
                cx,
                |_, cx| cx.quit(),
            ))
    }
}

// ── Shared row shapes, which `stats` draws with too ───────────────────────────

/// The frame every row in the popover shares: the menu's text inset and a
/// couple of pixels of air above and below.
pub fn row_frame() -> Div {
    div()
        .flex()
        .flex_row()
        .px(theme::ROW_PAD_X)
        .py(theme::ROW_PAD_Y)
}

/// A bold section header ("Claude Usage", "Today"). It takes no colour: the
/// header is primary text, which the root already sets.
pub fn section_header(label: &'static str) -> impl IntoElement {
    div()
        .px(theme::ROW_PAD_X)
        .pt(px(SECTION_HEADER_PAD_TOP))
        .pb(px(SECTION_HEADER_PAD_BOTTOM))
        // Bold, not semibold: the Battery menu's "Battery" is set in the
        // system bold, and semibold beside it looks washed out.
        .font_weight(FontWeight::BOLD)
        .text_size(theme::TEXT_TITLE)
        .line_height(theme::LINE_TITLE)
        .child(label)
}

/// The small tertiary label over a group of settings rows.
fn section_label(theme: Theme, label: &'static str) -> impl IntoElement {
    div()
        .px(theme::ROW_PAD_X + theme::ROW_INDENT)
        .pt(px(SECTION_LABEL_PAD_TOP))
        .pb(px(SECTION_LABEL_PAD_BOTTOM))
        .text_size(theme::TEXT_MICRO)
        .line_height(theme::LINE_MICRO)
        .text_color(theme.tertiary)
        .child(label)
}

/// A menu separator: a hairline inset from both edges with air around it.
fn separator(theme: Theme) -> Div {
    div()
        .h(theme::HAIRLINE)
        .mx(theme::SEPARATOR_INSET)
        .my(theme::SEPARATOR_MARGIN)
        .bg(theme.separator)
}

// ── Layout constants (see `docs/mockup-popover-v2.dc.html`) ──────────────────

/// The mockup's `4px 10px 2px` header padding around a 13px line.
const SECTION_HEADER_PAD_TOP: f32 = 4.0;
const SECTION_HEADER_PAD_BOTTOM: f32 = 2.0;
pub const SECTION_HEADER_HEIGHT: f32 =
    SECTION_HEADER_PAD_TOP + theme::LINE_TITLE_PX + SECTION_HEADER_PAD_BOTTOM;
/// One row: the mockup's 3px of vertical padding around a 13px line.
pub const ROW_HEIGHT: f32 = theme::ROW_PAD_Y_PX * 2.0 + theme::LINE_TITLE_PX;
/// A separator and the air above and below it.
pub const SEPARATOR_HEIGHT: f32 = theme::HAIRLINE_PX + theme::SEPARATOR_MARGIN_PX * 2.0;
/// The popover's own inset, top and bottom.
const POPOVER_PAD_TOTAL: f32 = theme::POPOVER_PAD_PX * 2.0;
/// The mockup's `2px 10px 6px` around the limit blocks.
const LIMITS_PAD_TOP: f32 = 2.0;
const LIMITS_PAD_BOTTOM: f32 = 6.0;
/// One limit: the label row, the bar and the reset subtitle, 4px apart.
const LIMIT_BLOCK_HEIGHT: f32 = theme::LINE_TITLE_PX
    + theme::LIMIT_GAP_PX
    + theme::LIMIT_BAR_HEIGHT_PX
    + theme::LIMIT_GAP_PX
    + theme::LINE_TINY_PX;
/// The muted status line under the header.
pub const NOTICE_HEIGHT: f32 = theme::LINE_TINY_PX;
/// The label over the limit rows in the Settings section.
const MENU_BAR_SECTION: &str = "Menu bar shows";

/// One of the "how it draws them" rows under the limit list.
#[derive(Debug, Clone, Copy)]
struct DrawingRow {
    /// Both the row's element id and which setting it flips.
    id: &'static str,
    label: &'static str,
    checked: bool,
    /// False for the last choice that is keeping ink in the item: the row
    /// still shows its checkmark, it just cannot be cleared.
    enabled: bool,
}
const SECTION_LABEL_PAD_TOP: f32 = 6.0;
const SECTION_LABEL_PAD_BOTTOM: f32 = 2.0;
const SECTION_LABEL_HEIGHT: f32 =
    SECTION_LABEL_PAD_TOP + theme::LINE_MICRO_PX + SECTION_LABEL_PAD_BOTTOM;
/// A note under a row (no bundle, or a failed toggle).
const NOTE_PAD_BOTTOM: f32 = 4.0;
const NOTE_HEIGHT: f32 = theme::LINE_MICRO_PX + NOTE_PAD_BOTTOM;
/// What stands in for the limit blocks with no token, and while the first
/// fetch is still out.
const SIGNED_OUT_LINE: &str = "Sign in with `claude` in a terminal first";
const LOADING_LINE: &str = "Checking your limits…";
const NO_NUMBERS_LINE: &str = "No numbers yet";

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

        let today = stats::today(self, cx);
        let week = stats::week(self, cx);

        div()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Refresh, _, cx| this.refresh(cx)))
            .on_action(cx.listener(Self::on_dismiss))
            .flex()
            .flex_col()
            .w(theme::POPOVER_WIDTH)
            .h_full()
            .p(theme::POPOVER_PAD)
            .bg(theme.bg)
            .rounded(theme::POPOVER_RADIUS)
            .border_1()
            .border_color(theme.border)
            .overflow_hidden()
            .font_family(theme::UI_FAMILY)
            .text_size(theme::TEXT_BODY)
            .text_color(theme.text)
            .child(section_header("Claude Usage"))
            .children(self.notice_line())
            .child(self.limits_block())
            .child(separator(theme))
            .child(today)
            .child(separator(theme))
            .child(week)
            .child(separator(theme))
            .child(self.menu_rows(cx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The heights the window is sized from, exercised without a gpui window:
    /// they are plain arithmetic over the section constants.
    fn total(limits: f32, notice: f32, menu: f32) -> f32 {
        POPOVER_PAD_TOTAL
            + SECTION_HEADER_HEIGHT
            + notice
            + limits
            + SEPARATOR_HEIGHT
            + stats::today_height()
            + SEPARATOR_HEIGHT
            + stats::week_height()
            + SEPARATOR_HEIGHT
            + menu
    }

    /// Three limits, no notice, the menu collapsed and in a bundle.
    fn three_limits() -> f32 {
        LIMITS_PAD_TOP
            + 3.0 * LIMIT_BLOCK_HEIGHT
            + 2.0 * theme::LIMIT_BLOCK_GAP_PX
            + LIMITS_PAD_BOTTOM
    }

    fn collapsed_menu() -> f32 {
        5.0 * ROW_HEIGHT + SEPARATOR_HEIGHT
    }

    #[test]
    fn the_rows_are_the_mockups_row() {
        assert_eq!(ROW_HEIGHT, 23.0);
        assert_eq!(SECTION_HEADER_HEIGHT, 23.0);
        // A 1px rule with 5px of margin either side.
        assert_eq!(SEPARATOR_HEIGHT, 11.0);
    }

    #[test]
    fn a_limit_block_is_its_row_bar_and_subtitle() {
        // 17 + 4 + 4 + 4 + 14.
        assert_eq!(LIMIT_BLOCK_HEIGHT, 43.0);
        assert_eq!(three_limits(), 2.0 + 129.0 + 16.0 + 6.0);
    }

    #[test]
    fn the_ready_popover_is_the_mockups_height() {
        // 10 padding, 23 header, 153 limits, 92 Today, 23 week, 3 separators,
        // 126 of menu rows.
        assert_eq!(total(three_limits(), 0.0, collapsed_menu()), 460.0);
    }

    /// The notice line and the expanded Settings section both add height, and
    /// the signed-out line takes far less than three limit blocks.
    #[test]
    fn the_states_change_the_total() {
        let ready = total(three_limits(), 0.0, collapsed_menu());
        assert_eq!(
            total(three_limits(), NOTICE_HEIGHT, collapsed_menu()) - ready,
            theme::LINE_TINY_PX
        );
        let signed_out = LIMITS_PAD_TOP + theme::LINE_SMALL_PX + LIMITS_PAD_BOTTOM;
        assert!(total(signed_out, 0.0, collapsed_menu()) < ready);
        let expanded = collapsed_menu() + SECTION_LABEL_HEIGHT + 4.0 * ROW_HEIGHT;
        assert!(total(three_limits(), 0.0, expanded) > ready);
    }

    #[test]
    fn the_usage_page_is_the_settings_tab() {
        assert_eq!(USAGE_URL, "https://claude.ai/settings/usage");
    }
}
