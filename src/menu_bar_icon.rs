//! Placeholder; written in the build phase.

use objc2::rc::Retained;
use objc2_app_kit::NSImage;

/// Placeholder so the scaffold compiles; replaced by the ring image.
pub fn envelope_image() -> Retained<NSImage> {
    NSImage::new()
}
