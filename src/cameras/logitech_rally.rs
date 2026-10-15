use anyhow::{bail, Context, Result};
use std::os::unix::io::RawFd;

use super::traits::{CameraCapabilities, CameraProfile, PtzLimits};
use crate::v4l2::constants::{
    V4L2_CID_PAN_ABSOLUTE, V4L2_CID_TILT_ABSOLUTE, V4L2_CID_ZOOM_ABSOLUTE,
};
use crate::v4l2::ioctl::{
    ioctl_call, UvcXuControlQuery, V4l2Control, UVCIOC_CTRL_QUERY, UVC_SET_CUR, VIDIOC_S_CTRL,
};

// Logitech USB Vendor ID & Product IDs
pub const LOGITECH_VENDOR_ID: u16 = 0x046d;
pub const RALLY_CAMERA_PRODUCT_ID: u16 = 0x0881;
pub const RALLY_CAMERA_PID_ALT1: u16 = 0x085e;
pub const RALLY_CAMERA_PID_ALT2: u16 = 0x085c;

// Logitech Rally default step and degree conversion:
// 1 degree = 3600 arcseconds (units)
pub const UNITS_PER_DEGREE: f32 = 3600.0;

// Logi Rally Limits (confirmed on hardware)
pub const RALLY_PAN_MIN: i32 = -324000; // -90.0°
pub const RALLY_PAN_MAX: i32 = 324000; // +90.0°
pub const RALLY_PAN_STEP: i32 = 3600; // 1.0°

pub const RALLY_TILT_MIN: i32 = -324000; // -90.0°
pub const RALLY_TILT_MAX: i32 = 180000; // +50.0°
pub const RALLY_TILT_STEP: i32 = 3600; // 1.0°

pub const RALLY_ZOOM_MIN: i32 = 100; // 1.0x
pub const RALLY_ZOOM_MAX: i32 = 1500; // 15.0x
pub const RALLY_ZOOM_STEP: i32 = 1;

// Logitech UVC Extension Unit (XU) definitions
// Unit ID 11 corresponds to LOGITECH_XU_PERIPHERAL_CONTROL {FFE52D21-8030-4E2C-82D9-F587D00540BD}
pub const LOGI_XU_PERIPHERAL_UNIT_ID: u8 = 11;
pub const XU_PERIPHERALCONTROL_PANTILT_RELATIVE_CONTROL: u8 = 0x01;
pub const XU_PERIPHERALCONTROL_PANTILT_MODE_CONTROL: u8 = 0x02;

// Unit ID 10 corresponds to LOGITECH_XU_VIDEOPIPE_CONTROL {49E40215-F434-47FE-B158-0E885023E51B}
pub const LOGI_XU_VIDEOPIPE_UNIT_ID: u8 = 10;
pub const XU_VIDEO_FW_ZOOM_CONTROL: u8 = 0x06;

// Mode control commands for XU_PERIPHERALCONTROL_PANTILT_MODE_CONTROL (Selector 2)
pub const LOGI_XU_CMD_RESET_PAN: u8 = 1;
pub const LOGI_XU_CMD_RESET_TILT: u8 = 2;
pub const LOGI_XU_CMD_HOME: u8 = 3;
pub const LOGI_XU_CMD_PRESET_SAVE_BASE: u8 = 4; // Presets 1..8 save: 4..11
pub const LOGI_XU_CMD_PRESET_RECALL_BASE: u8 = 12; // Presets 1..8 recall: 12..19

// Quirk bounds: Selector 2 safe values (1..=19)
pub const SELECTOR_2_SAFE_MIN: u8 = 1;
pub const SELECTOR_2_SAFE_MAX: u8 = 19;

/// Check if a Selector 2 command value is within valid quirk bounds.
#[inline]
pub fn is_selector_2_safe(cmd: u8) -> bool {
    (SELECTOR_2_SAFE_MIN..=SELECTOR_2_SAFE_MAX).contains(&cmd)
}

#[inline]
pub fn raw_to_deg(raw: i32) -> f32 {
    raw as f32 / UNITS_PER_DEGREE
}

#[inline]
pub fn deg_to_raw(deg: f32) -> i32 {
    (deg * UNITS_PER_DEGREE).round() as i32
}

#[inline]
pub fn zoom_raw_to_x(raw: i32) -> f32 {
    raw as f32 / 100.0
}

#[inline]
pub fn zoom_x_to_raw(x: f32) -> i32 {
    (x * 100.0).round() as i32
}

/// Profile for Logitech Rally and Rally Plus PTZ cameras.
#[derive(Debug, Default, Clone)]
pub struct LogitechRallyProfile;

impl LogitechRallyProfile {
    pub fn new() -> Self {
        Self
    }

    /// Execute a command on Logitech XU Unit 11 Selector 2 with safe quirk bounds checking.
    pub fn execute_xu_cmd(&self, raw_fd: RawFd, cmd: u8) -> Result<()> {
        if !is_selector_2_safe(cmd) {
            bail!(
                "Quirk bounds violation: Logitech XU Selector 2 command {} is outside safe bounds ({}..={})",
                cmd,
                SELECTOR_2_SAFE_MIN,
                SELECTOR_2_SAFE_MAX
            );
        }

        let mut val = cmd;
        let mut query = UvcXuControlQuery {
            unit: LOGI_XU_PERIPHERAL_UNIT_ID,
            selector: XU_PERIPHERALCONTROL_PANTILT_MODE_CONTROL,
            query: UVC_SET_CUR,
            size: 1,
            data: &mut val as *mut u8,
        };

        unsafe {
            ioctl_call(raw_fd, UVCIOC_CTRL_QUERY, &mut query).with_context(|| {
                format!("Failed to send Logitech XU Selector 2 command {}", cmd)
            })?;
        }
        Ok(())
    }

    fn set_v4l2_control(&self, raw_fd: RawFd, cid: u32, val: i32) -> Result<()> {
        let mut ctrl = V4l2Control {
            id: cid,
            value: val,
        };
        unsafe {
            ioctl_call(raw_fd, VIDIOC_S_CTRL, &mut ctrl)
                .with_context(|| format!("Failed to set V4L2 control 0x{:08x} to {}", cid, val))?;
        }
        Ok(())
    }
}

impl CameraProfile for LogitechRallyProfile {
    fn name(&self) -> &str {
        "Logitech Rally"
    }

    fn capabilities(&self) -> CameraCapabilities {
        CameraCapabilities {
            has_pan_tilt: true,
            has_zoom: true,
            has_focus: true,
            has_hardware_presets: true,
            hardware_preset_count: 8,
            has_anti_flicker: true,
            has_privacy_mode: true,
            vendor_name: "Logitech".to_string(),
            model_name: "Rally Camera".to_string(),
        }
    }

    fn limits(&self) -> PtzLimits {
        PtzLimits {
            pan_min: RALLY_PAN_MIN,
            pan_max: RALLY_PAN_MAX,
            pan_step: RALLY_PAN_STEP,
            tilt_min: RALLY_TILT_MIN,
            tilt_max: RALLY_TILT_MAX,
            tilt_step: RALLY_TILT_STEP,
            zoom_min: RALLY_ZOOM_MIN,
            zoom_max: RALLY_ZOOM_MAX,
            zoom_step: RALLY_ZOOM_STEP,
        }
    }

    fn matches(&self, card_name: &str, usb_vid_pid: Option<(u16, u16)>) -> bool {
        if let Some((vid, pid)) = usb_vid_pid {
            if vid == LOGITECH_VENDOR_ID
                && (pid == RALLY_CAMERA_PRODUCT_ID
                    || pid == RALLY_CAMERA_PID_ALT1
                    || pid == RALLY_CAMERA_PID_ALT2)
            {
                return true;
            }
        }
        card_name.to_lowercase().contains("rally")
    }

    fn home(&self, raw_fd: RawFd) -> Result<()> {
        let _ = self.execute_xu_cmd(raw_fd, LOGI_XU_CMD_HOME);
        let _ = self.set_v4l2_control(raw_fd, V4L2_CID_PAN_ABSOLUTE, 0);
        let _ = self.set_v4l2_control(raw_fd, V4L2_CID_TILT_ABSOLUTE, 0);
        let _ = self.set_v4l2_control(raw_fd, V4L2_CID_ZOOM_ABSOLUTE, RALLY_ZOOM_MIN);
        Ok(())
    }

    fn recall_hardware_preset(&self, raw_fd: RawFd, preset_num: u8) -> Result<()> {
        if !(1..=8).contains(&preset_num) {
            bail!(
                "Logitech Rally preset number must be between 1 and 8 (got {})",
                preset_num
            );
        }
        let cmd = LOGI_XU_CMD_PRESET_RECALL_BASE + (preset_num - 1);
        self.execute_xu_cmd(raw_fd, cmd)
    }

    fn save_hardware_preset(&self, raw_fd: RawFd, preset_num: u8) -> Result<()> {
        if !(1..=8).contains(&preset_num) {
            bail!(
                "Logitech Rally preset number must be between 1 and 8 (got {})",
                preset_num
            );
        }
        let cmd = LOGI_XU_CMD_PRESET_SAVE_BASE + (preset_num - 1);
        self.execute_xu_cmd(raw_fd, cmd)
    }

    fn sleep_privacy(&self, raw_fd: RawFd) -> Result<()> {
        // Tilt lens fully downward into the base (-90.0°), center pan, reset zoom
        let _ = self.set_v4l2_control(raw_fd, V4L2_CID_PAN_ABSOLUTE, 0);
        self.set_v4l2_control(raw_fd, V4L2_CID_TILT_ABSOLUTE, RALLY_TILT_MIN)?;
        let _ = self.set_v4l2_control(raw_fd, V4L2_CID_ZOOM_ABSOLUTE, RALLY_ZOOM_MIN);
        Ok(())
    }

    fn wake_privacy(&self, raw_fd: RawFd) -> Result<()> {
        // Restore camera to Home position
        self.home(raw_fd)
    }
}
