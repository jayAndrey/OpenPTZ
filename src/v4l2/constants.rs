#![allow(dead_code)]

// V4L2 Control IDs (from linux/v4l2-controls.h)
pub const V4L2_CID_BASE: u32 = 0x00980900;
pub const V4L2_CID_USER_BASE: u32 = 0x00980900;
pub const V4L2_CID_BRIGHTNESS: u32 = V4L2_CID_BASE;
pub const V4L2_CID_CONTRAST: u32 = V4L2_CID_BASE + 1;
pub const V4L2_CID_SATURATION: u32 = V4L2_CID_BASE + 2;
pub const V4L2_CID_HUE: u32 = V4L2_CID_BASE + 3;
pub const V4L2_CID_AUTO_WHITE_BALANCE: u32 = V4L2_CID_BASE + 12;
pub const V4L2_CID_GAIN: u32 = V4L2_CID_BASE + 19;
pub const V4L2_CID_POWER_LINE_FREQUENCY: u32 = V4L2_CID_BASE + 24;
pub const V4L2_CID_WHITE_BALANCE_TEMPERATURE: u32 = V4L2_CID_BASE + 26;
pub const V4L2_CID_SHARPNESS: u32 = V4L2_CID_BASE + 27;
pub const V4L2_CID_BACKLIGHT_COMPENSATION: u32 = V4L2_CID_BASE + 28;

// Camera Class Control IDs
pub const V4L2_CID_CAMERA_CLASS_BASE: u32 = 0x009a0900;
pub const V4L2_CID_EXPOSURE_AUTO: u32 = V4L2_CID_CAMERA_CLASS_BASE + 1;
pub const V4L2_CID_EXPOSURE_ABSOLUTE: u32 = V4L2_CID_CAMERA_CLASS_BASE + 2;
pub const V4L2_CID_EXPOSURE_AUTO_PRIORITY: u32 = V4L2_CID_CAMERA_CLASS_BASE + 3;
pub const V4L2_CID_PAN_RELATIVE: u32 = V4L2_CID_CAMERA_CLASS_BASE + 4;
pub const V4L2_CID_TILT_RELATIVE: u32 = V4L2_CID_CAMERA_CLASS_BASE + 5;
pub const V4L2_CID_PAN_RESET: u32 = V4L2_CID_CAMERA_CLASS_BASE + 6;
pub const V4L2_CID_TILT_RESET: u32 = V4L2_CID_CAMERA_CLASS_BASE + 7;
pub const V4L2_CID_PAN_ABSOLUTE: u32 = V4L2_CID_CAMERA_CLASS_BASE + 8;
pub const V4L2_CID_TILT_ABSOLUTE: u32 = V4L2_CID_CAMERA_CLASS_BASE + 9;
pub const V4L2_CID_FOCUS_ABSOLUTE: u32 = V4L2_CID_CAMERA_CLASS_BASE + 10;
pub const V4L2_CID_FOCUS_RELATIVE: u32 = V4L2_CID_CAMERA_CLASS_BASE + 11;
pub const V4L2_CID_FOCUS_AUTO: u32 = V4L2_CID_CAMERA_CLASS_BASE + 12;
pub const V4L2_CID_ZOOM_ABSOLUTE: u32 = V4L2_CID_CAMERA_CLASS_BASE + 13;
pub const V4L2_CID_ZOOM_RELATIVE: u32 = V4L2_CID_CAMERA_CLASS_BASE + 14;
pub const V4L2_CID_ZOOM_CONTINUOUS: u32 = V4L2_CID_CAMERA_CLASS_BASE + 15;
pub const V4L2_CID_PAN_SPEED: u32 = V4L2_CID_CAMERA_CLASS_BASE + 32;
pub const V4L2_CID_TILT_SPEED: u32 = V4L2_CID_CAMERA_CLASS_BASE + 33;

// Logitech UVC Extension Unit (XU) definitions
// Unit ID 11 corresponds to LOGITECH_XU_PERIPHERAL_CONTROL {FFE52D21-8030-4E2C-82D9-F587D00540BD}
pub const LOGI_XU_PERIPHERAL_UNIT_ID: u8 = 11;
pub const XU_PERIPHERALCONTROL_PANTILT_RELATIVE_CONTROL: u8 = 0x01;
pub const XU_PERIPHERALCONTROL_PANTILT_MODE_CONTROL: u8 = 0x02;

// Unit ID 10 corresponds to LOGITECH_XU_VIDEOPIPE_CONTROL {49E40215-F434-47FE-B158-0E885023E51B}
pub const LOGI_XU_VIDEOPIPE_UNIT_ID: u8 = 10;
pub const XU_VIDEO_FW_ZOOM_CONTROL: u8 = 0x06;

// Mode control commands for XU_PERIPHERALCONTROL_PANTILT_MODE_CONTROL (Selector 2 is PANTILT_RESET)
// 1 = Reset Pan, 2 = Reset Tilt, 3 = Reset Both (Home)
pub const LOGI_XU_CMD_RESET_PAN: u8 = 1;
pub const LOGI_XU_CMD_RESET_TILT: u8 = 2;
pub const LOGI_XU_CMD_HOME: u8 = 3;

// Power Line Frequency (Anti-flicker) settings
pub const FLICKER_DISABLED: i32 = 0;
pub const FLICKER_50HZ: i32 = 1;
pub const FLICKER_60HZ: i32 = 2;
pub const FLICKER_AUTO: i32 = 3;

// Logitech Rally default step and degree conversion:
// 1 degree = 3600 arcseconds (units)
pub const UNITS_PER_DEGREE: f32 = 3600.0;

// Logi Rally Limits (confirmed on hardware)
pub const RALLY_PAN_MIN: i32 = -324000;  // -90.0°
pub const RALLY_PAN_MAX: i32 = 324000;   // +90.0°
pub const RALLY_PAN_STEP: i32 = 3600;    // 1.0°

pub const RALLY_TILT_MIN: i32 = -324000; // -90.0°
pub const RALLY_TILT_MAX: i32 = 180000;  // +50.0°
pub const RALLY_TILT_STEP: i32 = 3600;   // 1.0°

pub const RALLY_ZOOM_MIN: i32 = 100;     // 1.0x
pub const RALLY_ZOOM_MAX: i32 = 1500;    // 15.0x
pub const RALLY_ZOOM_STEP: i32 = 1;

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
