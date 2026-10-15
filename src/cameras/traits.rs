use serde::{Deserialize, Serialize};
use std::os::unix::io::RawFd;

/// PTZ coordinate limits and mapping functions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PtzLimits {
    pub pan_min: i32,
    pub pan_max: i32,
    pub pan_step: i32,
    pub tilt_min: i32,
    pub tilt_max: i32,
    pub tilt_step: i32,
    pub zoom_min: i32,
    pub zoom_max: i32,
    pub zoom_step: i32,
}

impl PtzLimits {
    /// Convert raw pan value to degrees.
    #[inline]
    pub fn pan_raw_to_deg(&self, raw: i32) -> f32 {
        raw as f32 / 3600.0
    }

    /// Convert degrees to raw pan value, clamped to [pan_min, pan_max].
    #[inline]
    pub fn deg_to_pan_raw(&self, deg: f32) -> i32 {
        let min_deg = self.pan_raw_to_deg(self.pan_min);
        let max_deg = self.pan_raw_to_deg(self.pan_max);
        let clamped = deg.clamp(min_deg, max_deg);
        ((clamped * 3600.0).round() as i32).clamp(self.pan_min, self.pan_max)
    }

    /// Convert raw tilt value to degrees.
    #[inline]
    pub fn tilt_raw_to_deg(&self, raw: i32) -> f32 {
        raw as f32 / 3600.0
    }

    /// Convert degrees to raw tilt value, clamped to [tilt_min, tilt_max].
    #[inline]
    pub fn deg_to_tilt_raw(&self, deg: f32) -> i32 {
        let min_deg = self.tilt_raw_to_deg(self.tilt_min);
        let max_deg = self.tilt_raw_to_deg(self.tilt_max);
        let clamped = deg.clamp(min_deg, max_deg);
        ((clamped * 3600.0).round() as i32).clamp(self.tilt_min, self.tilt_max)
    }

    /// Convert raw zoom value to multiplier factor (e.g. 1.0x to 15.0x).
    #[inline]
    pub fn zoom_raw_to_x(&self, raw: i32) -> f32 {
        raw as f32 / 100.0
    }

    /// Convert multiplier factor to raw zoom value, clamped to [zoom_min, zoom_max].
    #[inline]
    pub fn zoom_x_to_raw(&self, x: f32) -> i32 {
        let min_x = self.zoom_raw_to_x(self.zoom_min);
        let max_x = self.zoom_raw_to_x(self.zoom_max);
        let clamped = x.clamp(min_x, max_x);
        ((clamped * 100.0).round() as i32).clamp(self.zoom_min, self.zoom_max)
    }
}

/// Feature capabilities of a camera model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraCapabilities {
    pub has_pan_tilt: bool,
    pub has_zoom: bool,
    pub has_focus: bool,
    pub has_hardware_presets: bool,
    pub hardware_preset_count: u8,
    pub has_anti_flicker: bool,
    pub has_privacy_mode: bool,
    pub vendor_name: String,
    pub model_name: String,
}

impl Default for CameraCapabilities {
    fn default() -> Self {
        Self {
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
}

/// Trait defining the profile and vendor hooks for a specific camera model.
pub trait CameraProfile: Send + Sync {
    /// Human-readable profile name.
    fn name(&self) -> &str;

    /// Supported hardware capabilities.
    fn capabilities(&self) -> CameraCapabilities;

    /// Hardware motion and optical limits.
    fn limits(&self) -> PtzLimits;

    /// Returns true if this profile matches the specified V4L2 card name and optional USB VID/PID.
    fn matches(&self, card_name: &str, usb_vid_pid: Option<(u16, u16)>) -> bool;

    /// Hook executed immediately after opening the V4L2 device file descriptor.
    fn on_open(&self, raw_fd: RawFd) -> anyhow::Result<()> {
        let _ = raw_fd;
        Ok(())
    }

    /// Reset camera to hardware home position.
    fn home(&self, raw_fd: RawFd) -> anyhow::Result<()> {
        let _ = raw_fd;
        Ok(())
    }

    /// Recall a hardware EEPROM preset (1..8).
    fn recall_hardware_preset(&self, raw_fd: RawFd, preset_num: u8) -> anyhow::Result<()> {
        let _ = raw_fd;
        let _ = preset_num;
        anyhow::bail!("Hardware presets are not supported by this camera profile")
    }

    /// Save current position to a hardware EEPROM preset (1..8).
    fn save_hardware_preset(&self, raw_fd: RawFd, preset_num: u8) -> anyhow::Result<()> {
        let _ = raw_fd;
        let _ = preset_num;
        anyhow::bail!("Hardware presets are not supported by this camera profile")
    }

    /// Move camera out of privacy/standby mode.
    fn wake_privacy(&self, raw_fd: RawFd) -> anyhow::Result<()> {
        let _ = raw_fd;
        Ok(())
    }

    /// Move camera into privacy/sleep mode (e.g. lens tilted fully downward).
    fn sleep_privacy(&self, raw_fd: RawFd) -> anyhow::Result<()> {
        let _ = raw_fd;
        Ok(())
    }
}
