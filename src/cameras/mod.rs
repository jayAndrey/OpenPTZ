#![allow(unused_imports)]

pub mod generic;
pub mod logitech_rally;
pub mod registry;
pub mod traits;

#[cfg(test)]
mod tests;

pub use generic::GenericUvcProfile;
pub use logitech_rally::LogitechRallyProfile;
pub use registry::CameraRegistry;
pub use traits::{CameraCapabilities, CameraProfile, PtzLimits};

use std::sync::Arc;

/// Detect camera profile for a given card name and optional USB VID/PID.
pub fn detect_profile(card_name: &str, usb_vid_pid: Option<(u16, u16)>) -> Arc<dyn CameraProfile> {
    CameraRegistry::detect(card_name, usb_vid_pid)
}
