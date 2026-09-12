//! Dev preview: renders the popover in a normal window against `StubProvider`.
//!
//! `cargo run --example popover_preview` — the healthy popover by default. The
//! first argument picks a mode so each state can be screenshotted without a
//! token and without waiting for a fetch:
//!
//! ```text
//! cargo run --example popover_preview -- ready|nearly-out|signed-out|loading|error|settings
//! ```
//!
//! `settings` is `ready` with the Settings section already expanded: the
//! preview window takes no clicks, so the only way to see (and screenshot) it
//! open is to start it that way.

use std::rc::Rc;

use claudebar::ui::popover::{self, Popover, PopoverEvent};
use claudebar::ui::provider::StubProvider;
use gpui::{
    div, point, px, size, App, AppContext, Application, Bounds, Focusable, IntoElement,
    ParentElement, Render, Styled, TitlebarOptions, Window, WindowBounds, WindowOptions,
};

/// A frame around the popover so its rounded corners and shadowless edge are
/// visible against something that is not the popover's own background.
struct Preview {
    popover: gpui::Entity<Popover>,
}

impl Render for Preview {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // The real window is sized from `preferred_height`, so the preview does
        // the same: it shows the popover at exactly the height the menu bar app
        // will give it, and a wrong constant shows up as clipping or slack.
        let height = self.popover.read(cx).preferred_height();
        div()
            .size_full()
            .flex()
            .justify_center()
            .items_start()
            .bg(gpui::rgb(0xd7d3cc))
            .p(px(24.))
            .child(div().h(height).child(self.popover.clone()))
    }
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "ready".into());
    let settings_open = mode == "settings";

    Application::new().run(move |cx: &mut App| {
        popover::bind_keys(cx);

        let bounds = Bounds {
            origin: point(px(120.), px(120.)),
            size: size(px(308.), px(700.)),
        };

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("claudebar preview".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                let popover = cx.new(|cx| {
                    let provider: Rc<StubProvider> = Rc::new(match mode.as_str() {
                        "nearly-out" => StubProvider::nearly_out(),
                        "signed-out" => StubProvider::signed_out(),
                        "loading" => StubProvider::loading(),
                        "error" => StubProvider::with_error("Offline"),
                        _ => StubProvider::new(),
                    });
                    let mut popover = Popover::with_provider(provider, cx);
                    if settings_open {
                        popover.open_settings(cx);
                    }
                    popover
                });

                cx.subscribe(&popover, |_, event, _| match event {
                    PopoverEvent::Close => println!("preview: popover asked to close"),
                    PopoverEvent::Refresh => println!("preview: refresh requested"),
                })
                .detach();

                window.focus(&popover.focus_handle(cx));
                cx.new(|_| Preview { popover })
            },
        )
        .expect("open preview window");

        cx.activate(true);
    });
}
