use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub path: PathBuf,
    pub card_name: String,
    pub driver: String,
    pub bus_info: String,
    pub is_rally: bool,
    pub supports_ptz: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlInfo {
    pub id: u32,
    pub name: String,
    pub ctrl_type: u32,
    pub min: i32,
    pub max: i32,
    pub step: i32,
    pub default_val: i32,
    pub current_val: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraStatus {
    pub device_path: String,
    pub card_name: String,
    pub pan_deg: f32,
    pub pan_raw: i32,
    pub tilt_deg: f32,
    pub tilt_raw: i32,
    pub zoom_x: f32,
    pub zoom_raw: i32,
    pub brightness: Option<i32>,
    pub contrast: Option<i32>,
    pub saturation: Option<i32>,
    pub sharpness: Option<i32>,
    pub power_line_frequency: Option<i32>,
    pub auto_exposure: Option<i32>,
    pub exposure_time: Option<i32>,
    pub auto_focus: Option<bool>,
    pub white_balance_temp: Option<i32>,
    pub auto_white_balance: Option<bool>,
}
