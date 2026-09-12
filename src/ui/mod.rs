//! gpui views for the popover.
//!
//! `popover` is the root view; `rings`, `stats` are the leaves it composes.
//! All colors and sizes come from [`theme`] — no literals in the views.

pub mod popover;
pub mod provider;
pub mod rings;
pub mod stats;
pub mod theme;
