#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use anyhow::{bail, Result};
use serde_json::Value;

use crate::camera::constants::*;
use crate::camera::types::*;
use crate::cameras::CameraProfile;

pub struct CameraDeviceImpl {
    path: PathBuf,
    card_name: String,
    device_index: usize,
    has_rally_xu: bool,
    has_uvc_util: bool,
    pub profile: Arc<dyn CameraProfile>,
    cached_pan_deg: Mutex<f32>,
    cached_tilt_deg: Mutex<f32>,
    cached_zoom_x: Mutex<f32>,
    brightness: Mutex<Option<i32>>,
    contrast: Mutex<Option<i32>>,
    saturation: Mutex<Option<i32>>,
    sharpness: Mutex<Option<i32>>,
    power_line_freq: Mutex<Option<i32>>,
    auto_exposure: Mutex<Option<i32>>,
    exposure_time: Mutex<Option<i32>>,
    auto_focus: Mutex<Option<bool>>,
}

fn check_uvc_util() -> bool {
    Command::new("which")
        .arg("uvc-util")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

impl CameraDeviceImpl {
    pub fn discover_devices() -> Vec<DeviceInfo> {
        let mut devices = Vec::new();

        // 1. Query macOS system_profiler for camera data
        if let Ok(output) = Command::new("system_profiler")
            .args(["SPCameraDataType", "-json"])
            .output()
        {
            if output.status.success() {
                if let Ok(json) = serde_json::from_slice::<Value>(&output.stdout) {
                    if let Some(items) = json.get("SPCameraDataType").and_then(|v| v.as_array()) {
                        for (idx, item) in items.iter().enumerate() {
                            let name = item
                                .get("_name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("Camera")
                                .to_string();

                            let model_id = item
                                .get("spcamera_model-id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();

                            let unique_id = item
                                .get("spcamera_unique-id")
                                .and_then(|v| v.as_str())
                                .unwrap_or(&format!("camera-{}", idx))
                                .to_string();

                            let is_rally = name.to_lowercase().contains("rally")
                                || model_id.to_lowercase().contains("rally");

                            let supports_ptz = is_rally || name.to_lowercase().contains("ptz");

                            devices.push(DeviceInfo {
                                path: PathBuf::from(&unique_id),
                                card_name: name,
                                driver: "AVFoundation / UVC".to_string(),
                                bus_info: format!("Index {}", idx),
                                is_rally,
                                supports_ptz,
                            });
                        }
                    }
                }
            }
        }

        // Fallback: If no cameras returned by system_profiler, detect default
        if devices.is_empty() {
            devices.push(DeviceInfo {
                path: PathBuf::from("default"),
                card_name: "Default macOS Video Camera".to_string(),
                driver: "AVFoundation".to_string(),
                bus_info: "Index 0".to_string(),
                is_rally: false,
                supports_ptz: false,
            });
        }

        devices
    }

    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_ref = path.as_ref();
        let path_str = path_ref.to_string_lossy().to_string();
        let devices = Self::discover_devices();

        let (idx, matched) = devices
            .iter()
            .enumerate()
            .find(|(i, d)| {
                d.path == path_ref
                    || path_str == i.to_string()
                    || d.card_name
                        .to_lowercase()
                        .contains(&path_str.to_lowercase())
            })
            .map(|(i, d)| (i, d.clone()))
            .unwrap_or((
                0,
                DeviceInfo {
                    path: path_ref.to_path_buf(),
                    card_name: path_str.clone(),
                    driver: "AVFoundation".to_string(),
                    bus_info: "Index 0".to_string(),
                    is_rally: path_str.to_lowercase().contains("rally"),
                    supports_ptz: path_str.to_lowercase().contains("ptz")
                        || path_str.to_lowercase().contains("rally"),
                },
            ));

        let has_uvc = check_uvc_util();
        let profile = crate::cameras::detect_profile(&matched.card_name, None);
        let has_rally_xu = profile.capabilities().vendor_name == "Logitech"
            || matched.card_name.to_lowercase().contains("rally");

        Ok(Self {
            path: matched.path,
            card_name: matched.card_name,
            device_index: idx,
            has_rally_xu,
            has_uvc_util: has_uvc,
            profile,
            cached_pan_deg: Mutex::new(0.0),
            cached_tilt_deg: Mutex::new(0.0),
            cached_zoom_x: Mutex::new(1.0),
            brightness: Mutex::new(Some(128)),
            contrast: Mutex::new(Some(128)),
            saturation: Mutex::new(Some(128)),
            sharpness: Mutex::new(Some(128)),
            power_line_freq: Mutex::new(Some(FLICKER_50HZ)),
            auto_exposure: Mutex::new(Some(3)),
            exposure_time: Mutex::new(Some(156)),
            auto_focus: Mutex::new(Some(true)),
        })
    }

    pub fn open_default() -> Result<Self> {
        let devs = Self::discover_devices();
        if devs.is_empty() {
            bail!("No video capture devices found on macOS system.");
        }

        if let Some(rally) = devs.iter().find(|d| d.is_rally) {
            return Self::open(&rally.path);
        }

        if let Some(ptz) = devs.iter().find(|d| d.supports_ptz) {
            return Self::open(&ptz.path);
        }

        Self::open(&devs[0].path)
    }

    pub fn card_name(&self) -> &str {
        &self.card_name
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn is_rally(&self) -> bool {
        self.profile.capabilities().vendor_name == "Logitech"
            || self.card_name.to_lowercase().contains("rally")
    }

    pub fn has_rally_xu(&self) -> bool {
        self.has_rally_xu
    }

    pub fn query_all_ctrls(&self) -> Vec<ControlInfo> {
        let limits = self.profile.limits();
        vec![
            ControlInfo {
                id: 1,
                name: "Pan (Absolute)".to_string(),
                ctrl_type: 1,
                min: limits.pan_min,
                max: limits.pan_max,
                step: limits.pan_step,
                default_val: 0,
                current_val: Some(limits.deg_to_pan_raw(*self.cached_pan_deg.lock().unwrap())),
            },
            ControlInfo {
                id: 2,
                name: "Tilt (Absolute)".to_string(),
                ctrl_type: 1,
                min: limits.tilt_min,
                max: limits.tilt_max,
                step: limits.tilt_step,
                default_val: 0,
                current_val: Some(limits.deg_to_tilt_raw(*self.cached_tilt_deg.lock().unwrap())),
            },
            ControlInfo {
                id: 3,
                name: "Zoom (Absolute)".to_string(),
                ctrl_type: 1,
                min: limits.zoom_min,
                max: limits.zoom_max,
                step: limits.zoom_step,
                default_val: limits.zoom_min,
                current_val: Some(limits.zoom_x_to_raw(*self.cached_zoom_x.lock().unwrap())),
            },
        ]
    }

    fn send_uvc_cmd(&self, ctrl: &str, val: &str) {
        if self.has_uvc_util {
            let _ = Command::new("uvc-util")
                .args(["-I", &self.device_index.to_string(), "-s", ctrl, val])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        }
    }

    pub fn get_pan_deg(&self) -> Result<f32> {
        Ok(*self.cached_pan_deg.lock().unwrap())
    }

    pub fn set_pan_deg(&self, deg: f32) -> Result<()> {
        let limits = self.profile.limits();
        let min_deg = limits.pan_raw_to_deg(limits.pan_min);
        let max_deg = limits.pan_raw_to_deg(limits.pan_max);
        let clamped = deg.clamp(min_deg, max_deg);
        *self.cached_pan_deg.lock().unwrap() = clamped;
        let tilt = *self.cached_tilt_deg.lock().unwrap();
        self.send_uvc_cmd(
            "pan-tilt-abs",
            &format!(
                "{},{}",
                limits.deg_to_pan_raw(clamped),
                limits.deg_to_tilt_raw(tilt)
            ),
        );
        Ok(())
    }

    #[inline]
    pub fn pan_to_deg(&self, deg: f32) -> Result<()> {
        self.set_pan_deg(deg)
    }

    pub fn pan_relative(&self, delta_deg: f32) -> Result<f32> {
        let cur = self.get_pan_deg()?;
        let target = cur + delta_deg;
        self.set_pan_deg(target)?;
        self.get_pan_deg()
    }

    pub fn get_tilt_deg(&self) -> Result<f32> {
        Ok(*self.cached_tilt_deg.lock().unwrap())
    }

    pub fn set_tilt_deg(&self, deg: f32) -> Result<()> {
        let limits = self.profile.limits();
        let min_deg = limits.tilt_raw_to_deg(limits.tilt_min);
        let max_deg = limits.tilt_raw_to_deg(limits.tilt_max);
        let clamped = deg.clamp(min_deg, max_deg);
        *self.cached_tilt_deg.lock().unwrap() = clamped;
        let pan = *self.cached_pan_deg.lock().unwrap();
        self.send_uvc_cmd(
            "pan-tilt-abs",
            &format!(
                "{},{}",
                limits.deg_to_pan_raw(pan),
                limits.deg_to_tilt_raw(clamped)
            ),
        );
        Ok(())
    }

    #[inline]
    pub fn tilt_to_deg(&self, deg: f32) -> Result<()> {
        self.set_tilt_deg(deg)
    }

    pub fn tilt_relative(&self, delta_deg: f32) -> Result<f32> {
        let cur = self.get_tilt_deg()?;
        let target = cur + delta_deg;
        self.set_tilt_deg(target)?;
        self.get_tilt_deg()
    }

    pub fn move_pan_tilt(&self, pan_deg: f32, tilt_deg: f32) -> Result<()> {
        self.set_pan_deg(pan_deg)?;
        self.set_tilt_deg(tilt_deg)?;
        Ok(())
    }

    pub fn get_zoom_x(&self) -> Result<f32> {
        Ok(*self.cached_zoom_x.lock().unwrap())
    }

    pub fn set_zoom_x(&self, x: f32) -> Result<()> {
        let limits = self.profile.limits();
        let min_x = limits.zoom_raw_to_x(limits.zoom_min);
        let max_x = limits.zoom_raw_to_x(limits.zoom_max);
        let clamped = x.clamp(min_x, max_x);
        *self.cached_zoom_x.lock().unwrap() = clamped;
        self.send_uvc_cmd("zoom-abs", &limits.zoom_x_to_raw(clamped).to_string());
        Ok(())
    }

    #[inline]
    pub fn zoom_to(&self, x: f32) -> Result<()> {
        self.set_zoom_x(x)
    }

    pub fn zoom_relative(&self, delta_x: f32) -> Result<f32> {
        let cur = self.get_zoom_x()?;
        let target = cur + delta_x;
        self.set_zoom_x(target)?;
        self.get_zoom_x()
    }

    pub fn set_pan_speed(&self, _speed: i32) -> Result<()> {
        Ok(())
    }

    pub fn set_tilt_speed(&self, _speed: i32) -> Result<()> {
        Ok(())
    }

    pub fn stop_motion(&self) -> Result<()> {
        Ok(())
    }

    pub fn home(&self) -> Result<()> {
        self.set_pan_deg(0.0)?;
        self.set_tilt_deg(0.0)?;
        self.set_zoom_x(1.0)?;
        Ok(())
    }

    #[inline]
    pub fn goto_home(&self) -> Result<()> {
        self.home()
    }

    pub fn recall_hardware_preset(&self, _preset_num: u8) -> Result<()> {
        anyhow::bail!("Hardware EEPROM presets not supported via generic macOS driver")
    }

    pub fn save_hardware_preset(&self, _preset_num: u8) -> Result<()> {
        anyhow::bail!("Hardware EEPROM presets not supported via generic macOS driver")
    }

    pub fn sleep_privacy(&self) -> Result<()> {
        self.set_tilt_deg(-90.0)?;
        Ok(())
    }

    pub fn wake_privacy(&self) -> Result<()> {
        self.home()
    }

    pub fn get_power_line_frequency(&self) -> Result<i32> {
        Ok(self.power_line_freq.lock().unwrap().unwrap_or(0))
    }

    pub fn set_power_line_frequency(&self, hz: u32) -> Result<()> {
        let val = match hz {
            0 => FLICKER_DISABLED,
            50 => FLICKER_50HZ,
            60 => FLICKER_60HZ,
            _ => bail!("Invalid power line frequency: {} (choose 0, 50, or 60)", hz),
        };
        *self.power_line_freq.lock().unwrap() = Some(val);
        self.send_uvc_cmd("power-line-frequency", &val.to_string());
        Ok(())
    }

    pub fn cycle_power_line_frequency(&self) -> Result<i32> {
        let current = self.get_power_line_frequency().unwrap_or(0);
        let next = match current {
            FLICKER_DISABLED => FLICKER_50HZ,
            FLICKER_50HZ => FLICKER_60HZ,
            _ => FLICKER_DISABLED,
        };
        self.set_power_line_frequency(next as u32)?;
        Ok(next)
    }

    pub fn get_brightness(&self) -> Result<i32> {
        self.brightness
            .lock()
            .unwrap()
            .ok_or_else(|| anyhow::anyhow!("Brightness not available"))
    }

    pub fn set_brightness(&self, val: i32) -> Result<()> {
        let clamped = val.clamp(0, 255);
        *self.brightness.lock().unwrap() = Some(clamped);
        self.send_uvc_cmd("brightness", &clamped.to_string());
        Ok(())
    }

    pub fn get_contrast(&self) -> Result<i32> {
        self.contrast
            .lock()
            .unwrap()
            .ok_or_else(|| anyhow::anyhow!("Contrast not available"))
    }

    pub fn set_contrast(&self, val: i32) -> Result<()> {
        let clamped = val.clamp(0, 255);
        *self.contrast.lock().unwrap() = Some(clamped);
        self.send_uvc_cmd("contrast", &clamped.to_string());
        Ok(())
    }

    pub fn get_saturation(&self) -> Result<i32> {
        self.saturation
            .lock()
            .unwrap()
            .ok_or_else(|| anyhow::anyhow!("Saturation not available"))
    }

    pub fn set_saturation(&self, val: i32) -> Result<()> {
        let clamped = val.clamp(0, 255);
        *self.saturation.lock().unwrap() = Some(clamped);
        self.send_uvc_cmd("saturation", &clamped.to_string());
        Ok(())
    }

    pub fn get_sharpness(&self) -> Result<i32> {
        self.sharpness
            .lock()
            .unwrap()
            .ok_or_else(|| anyhow::anyhow!("Sharpness not available"))
    }

    pub fn set_sharpness(&self, val: i32) -> Result<()> {
        let clamped = val.clamp(0, 255);
        *self.sharpness.lock().unwrap() = Some(clamped);
        self.send_uvc_cmd("sharpness", &clamped.to_string());
        Ok(())
    }

    pub fn get_auto_exposure(&self) -> Result<i32> {
        self.auto_exposure
            .lock()
            .unwrap()
            .ok_or_else(|| anyhow::anyhow!("Auto exposure not available"))
    }

    pub fn set_auto_exposure(&self, auto: bool) -> Result<()> {
        let val = if auto { 3 } else { 1 };
        *self.auto_exposure.lock().unwrap() = Some(val);
        self.send_uvc_cmd("exposure-auto", &val.to_string());
        Ok(())
    }

    pub fn get_exposure_time(&self) -> Result<i32> {
        self.exposure_time
            .lock()
            .unwrap()
            .ok_or_else(|| anyhow::anyhow!("Exposure time not available"))
    }

    pub fn set_exposure_time(&self, val: i32) -> Result<()> {
        *self.exposure_time.lock().unwrap() = Some(val);
        self.send_uvc_cmd("exposure-time-abs", &val.to_string());
        Ok(())
    }

    pub fn get_auto_focus(&self) -> Result<bool> {
        self.auto_focus
            .lock()
            .unwrap()
            .ok_or_else(|| anyhow::anyhow!("Auto focus not available"))
    }

    pub fn set_auto_focus(&self, auto: bool) -> Result<()> {
        *self.auto_focus.lock().unwrap() = Some(auto);
        self.send_uvc_cmd("focus-auto", if auto { "1" } else { "0" });
        Ok(())
    }

    pub fn get_status(&self) -> CameraStatus {
        let limits = self.profile.limits();
        let pan_deg = self.get_pan_deg().unwrap_or(0.0);
        let tilt_deg = self.get_tilt_deg().unwrap_or(0.0);
        let zoom_x = self.get_zoom_x().unwrap_or(1.0);

        CameraStatus {
            device_path: self.path.to_string_lossy().to_string(),
            card_name: self.card_name.clone(),
            pan_deg,
            pan_raw: limits.deg_to_pan_raw(pan_deg),
            tilt_deg,
            tilt_raw: limits.deg_to_tilt_raw(tilt_deg),
            zoom_x,
            zoom_raw: limits.zoom_x_to_raw(zoom_x),
            brightness: self.get_brightness().ok(),
            contrast: self.get_contrast().ok(),
            saturation: self.get_saturation().ok(),
            sharpness: self.get_sharpness().ok(),
            power_line_frequency: self.get_power_line_frequency().ok(),
            auto_exposure: self.get_auto_exposure().ok(),
            exposure_time: self.get_exposure_time().ok(),
            auto_focus: self.get_auto_focus().ok(),
            white_balance_temp: None,
            auto_white_balance: None,
        }
    }
}
