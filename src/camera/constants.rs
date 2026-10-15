#![allow(dead_code)]

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

// Power Line Frequency (Anti-flicker) settings
pub const FLICKER_DISABLED: i32 = 0;
pub const FLICKER_50HZ: i32 = 1;
pub const FLICKER_60HZ: i32 = 2;
pub const FLICKER_AUTO: i32 = 3;

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
