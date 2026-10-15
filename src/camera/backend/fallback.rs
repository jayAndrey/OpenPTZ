#![allow(dead_code)]

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::camera::constants::*;
use crate::camera::types::*;

pub struct CameraDeviceImpl {
    path: PathBuf,
    card_name: String,
    has_rally_xu: bool,
    cached_pan_deg: Mutex<f32>,
    cached_tilt_deg: Mutex<f32>,
    cached_zoom_x: Mutex<f32>,
}

impl CameraDeviceImpl {
    pub fn discover_devices() -> Vec<DeviceInfo> {
        vec![DeviceInfo {
            path: PathBuf::from("mock-ptz"),
            card_name: "Mock PTZ Camera".to_string(),
            driver: "MockDriver".to_string(),
            bus_info: "Virtual".to_string(),
            is_rally: true,
            supports_ptz: true,
        }]
    }

    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_ref = path.as_ref();
        Ok(Self {
            path: path_ref.to_path_buf(),
            card_name: "Mock PTZ Camera".to_string(),
            has_rally_xu: true,
            cached_pan_deg: Mutex::new(0.0),
            cached_tilt_deg: Mutex::new(0.0),
            cached_zoom_x: Mutex::new(1.0),
        })
    }

    pub fn open_default() -> Result<Self> {
        Self::open("mock-ptz")
    }

    pub fn card_name(&self) -> &str {
        &self.card_name
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn has_rally_xu(&self) -> bool {
        self.has_rally_xu
    }

    pub fn query_all_ctrls(&self) -> Vec<ControlInfo> {
        vec![]
    }

    pub fn get_pan_deg(&self) -> Result<f32> {
        Ok(*self.cached_pan_deg.lock().unwrap())
    }

    pub fn set_pan_deg(&self, deg: f32) -> Result<()> {
        *self.cached_pan_deg.lock().unwrap() = deg.clamp(-90.0, 90.0);
        Ok(())
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
        *self.cached_tilt_deg.lock().unwrap() = deg.clamp(-90.0, 50.0);
        Ok(())
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
        *self.cached_zoom_x.lock().unwrap() = x.clamp(1.0, 15.0);
        Ok(())
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

    pub fn goto_home(&self) -> Result<()> {
        self.set_pan_deg(0.0)?;
        self.set_tilt_deg(0.0)?;
        self.set_zoom_x(1.0)?;
        Ok(())
    }

    pub fn get_power_line_frequency(&self) -> Result<i32> {
        Ok(FLICKER_50HZ)
    }

    pub fn set_power_line_frequency(&self, _hz: u32) -> Result<()> {
        Ok(())
    }

    pub fn cycle_power_line_frequency(&self) -> Result<i32> {
        Ok(FLICKER_50HZ)
    }

    pub fn get_brightness(&self) -> Result<i32> {
        Ok(128)
    }

    pub fn set_brightness(&self, _val: i32) -> Result<()> {
        Ok(())
    }

    pub fn get_contrast(&self) -> Result<i32> {
        Ok(128)
    }

    pub fn set_contrast(&self, _val: i32) -> Result<()> {
        Ok(())
    }

    pub fn get_saturation(&self) -> Result<i32> {
        Ok(128)
    }

    pub fn set_saturation(&self, _val: i32) -> Result<()> {
        Ok(())
    }

    pub fn get_sharpness(&self) -> Result<i32> {
        Ok(128)
    }

    pub fn set_sharpness(&self, _val: i32) -> Result<()> {
        Ok(())
    }

    pub fn get_auto_exposure(&self) -> Result<i32> {
        Ok(3)
    }

    pub fn set_auto_exposure(&self, _auto: bool) -> Result<()> {
        Ok(())
    }

    pub fn get_exposure_time(&self) -> Result<i32> {
        Ok(156)
    }

    pub fn set_exposure_time(&self, _val: i32) -> Result<()> {
        Ok(())
    }

    pub fn get_auto_focus(&self) -> Result<bool> {
        Ok(true)
    }

    pub fn set_auto_focus(&self, _auto: bool) -> Result<()> {
        Ok(())
    }

    pub fn get_status(&self) -> CameraStatus {
        let pan_deg = self.get_pan_deg().unwrap_or(0.0);
        let tilt_deg = self.get_tilt_deg().unwrap_or(0.0);
        let zoom_x = self.get_zoom_x().unwrap_or(1.0);

        CameraStatus {
            device_path: self.path.to_string_lossy().to_string(),
            card_name: self.card_name.clone(),
            pan_deg,
            pan_raw: deg_to_raw(pan_deg),
            tilt_deg,
            tilt_raw: deg_to_raw(tilt_deg),
            zoom_x,
            zoom_raw: zoom_x_to_raw(zoom_x),
            brightness: Some(128),
            contrast: Some(128),
            saturation: Some(128),
            sharpness: Some(128),
            power_line_frequency: Some(FLICKER_50HZ),
            auto_exposure: Some(3),
            exposure_time: Some(156),
            auto_focus: Some(true),
            white_balance_temp: None,
            auto_white_balance: None,
        }
    }
}
