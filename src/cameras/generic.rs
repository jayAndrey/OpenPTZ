use super::traits::{CameraCapabilities, CameraProfile, PtzLimits};

/// Fallback profile for any standard V4L2 / UVC PTZ camera.
/// Uses standard UVC 3600-step arcsecond angles and software-only presets.
#[derive(Debug, Default, Clone)]
pub struct GenericUvcProfile;

impl GenericUvcProfile {
    pub fn new() -> Self {
        Self
    }
}

impl CameraProfile for GenericUvcProfile {
    fn name(&self) -> &str {
        "Generic UVC"
    }

    fn capabilities(&self) -> CameraCapabilities {
        CameraCapabilities {
            has_pan_tilt: true,
            has_zoom: true,
            has_focus: true,
            has_hardware_presets: false,
            hardware_preset_count: 0,
            has_anti_flicker: true,
            has_privacy_mode: false,
            vendor_name: "Generic".to_string(),
            model_name: "UVC PTZ Camera".to_string(),
        }
    }

    fn limits(&self) -> PtzLimits {
        PtzLimits {
            pan_min: -648000,  // -180.0°
            pan_max: 648000,   // +180.0°
            pan_step: 3600,    // 1.0°
            tilt_min: -324000, // -90.0°
            tilt_max: 324000,  // +90.0°
            tilt_step: 3600,   // 1.0°
            zoom_min: 100,     // 1.0x
            zoom_max: 1000,    // 10.0x
            zoom_step: 1,
        }
    }

    fn matches(&self, _card_name: &str, _usb_vid_pid: Option<(u16, u16)>) -> bool {
        // Fallback profile matches all cameras
        true
    }
}
