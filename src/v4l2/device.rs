#![allow(dead_code)]

use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use super::constants::*;
use super::ioctl::*;
use crate::cameras::CameraProfile;

pub use crate::camera::types::{CameraStatus, ControlInfo, DeviceInfo};

pub struct V4l2Device {
    file: File,
    path: PathBuf,
    card_name: String,
    has_rally_xu: bool,
    pub profile: Arc<dyn CameraProfile>,
}

pub type CameraDevice = V4l2Device;

impl V4l2Device {
    /// Read USB Vendor ID and Product ID from sysfs if device is a USB camera.
    pub fn read_usb_vid_pid(dev_path: &Path) -> Option<(u16, u16)> {
        let filename = dev_path.file_name()?.to_str()?;
        let sysfs = Path::new("/sys/class/video4linux")
            .join(filename)
            .join("device");
        for candidate in [
            sysfs.join("idVendor"),
            sysfs.join("../idVendor"),
            sysfs.join("../../idVendor"),
        ] {
            if candidate.exists() {
                let parent = candidate.parent()?;
                let vid_str = fs::read_to_string(parent.join("idVendor")).ok()?;
                let pid_str = fs::read_to_string(parent.join("idProduct")).ok()?;
                let vid = u16::from_str_radix(vid_str.trim(), 16).ok()?;
                let pid = u16::from_str_radix(pid_str.trim(), 16).ok()?;
                return Some((vid, pid));
            }
        }
        None
    }

    /// Discover all video devices on the system
    pub fn discover_devices() -> Vec<DeviceInfo> {
        let mut devices = Vec::new();

        // Check /sys/class/video4linux
        let sys_path = Path::new("/sys/class/video4linux");
        if let Ok(entries) = fs::read_dir(sys_path) {
            let mut paths: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
            paths.sort();

            for p in paths {
                let filename = match p.file_name() {
                    Some(f) => f.to_string_lossy().to_string(),
                    None => continue,
                };
                let dev_path = PathBuf::from(format!("/dev/{}", filename));
                if !dev_path.exists() {
                    continue;
                }

                if let Ok(dev) = Self::open_read_only(&dev_path) {
                    if let Ok(info) = dev.query_capability() {
                        // Check if this device node supports video capture
                        const V4L2_CAP_VIDEO_CAPTURE: u32 = 0x00000001;
                        if (info.device_caps & V4L2_CAP_VIDEO_CAPTURE) == 0
                            && (info.capabilities & V4L2_CAP_VIDEO_CAPTURE) == 0
                        {
                            continue;
                        }

                        let card = String::from_utf8_lossy(&info.card)
                            .trim_matches(char::from(0))
                            .to_string();
                        let driver = String::from_utf8_lossy(&info.driver)
                            .trim_matches(char::from(0))
                            .to_string();
                        let bus = String::from_utf8_lossy(&info.bus_info)
                            .trim_matches(char::from(0))
                            .to_string();

                        let is_rally = dev.is_rally();
                        let supports_ptz = is_rally
                            || dev.profile.capabilities().has_pan_tilt
                            || dev.get_ctrl(V4L2_CID_PAN_ABSOLUTE).is_ok()
                            || dev.get_ctrl(V4L2_CID_ZOOM_ABSOLUTE).is_ok();

                        devices.push(DeviceInfo {
                            path: dev_path,
                            card_name: card,
                            driver,
                            bus_info: bus,
                            is_rally,
                            supports_ptz,
                        });
                    }
                }
            }
        }

        devices
    }

    /// Open read-only helper for enumeration
    fn open_read_only(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
            .with_context(|| format!("Failed to open device {:?}", path))?;

        let raw_fd = file.as_raw_fd();
        let mut card_name = String::new();
        let mut cap = V4l2Capability::default();
        if unsafe { ioctl_call(raw_fd, VIDIOC_QUERYCAP, &mut cap) }.is_ok() {
            card_name = String::from_utf8_lossy(&cap.card)
                .trim_matches(char::from(0))
                .to_string();
        }

        let vid_pid = Self::read_usb_vid_pid(path);
        let profile = crate::cameras::detect_profile(&card_name, vid_pid);

        Ok(Self {
            file,
            path: path.to_path_buf(),
            card_name,
            has_rally_xu: false,
            profile,
        })
    }

    /// Open device for control (read + write)
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path_buf)
            .with_context(|| format!("Failed to open camera device {:?}", path_buf))?;

        let raw_fd = file.as_raw_fd();
        let mut card_name = String::new();
        let mut cap = V4l2Capability::default();
        if unsafe { ioctl_call(raw_fd, VIDIOC_QUERYCAP, &mut cap) }.is_ok() {
            card_name = String::from_utf8_lossy(&cap.card)
                .trim_matches(char::from(0))
                .to_string();
        }

        let vid_pid = Self::read_usb_vid_pid(&path_buf);
        let profile = crate::cameras::detect_profile(&card_name, vid_pid);

        // Run profile initialization hook
        profile.on_open(raw_fd)?;

        let mut dev = Self {
            file,
            path: path_buf,
            card_name,
            has_rally_xu: false,
            profile,
        };

        // Test Logitech XU peripheral mode control support
        dev.has_rally_xu = dev.check_xu_support();

        Ok(dev)
    }

    /// Automatically find and open the best candidate camera (Logi Rally first, or any PTZ camera)
    pub fn open_default() -> Result<Self> {
        let devs = Self::discover_devices();
        if devs.is_empty() {
            bail!("No V4L2 video devices found on system.");
        }

        // 1. Look for Logitech Rally first
        if let Some(rally) = devs.iter().find(|d| d.is_rally) {
            return Self::open(&rally.path);
        }

        // 2. Look for any camera supporting PTZ
        if let Some(ptz) = devs.iter().find(|d| d.supports_ptz) {
            return Self::open(&ptz.path);
        }

        // 3. Fallback to first device
        Self::open(&devs[0].path)
    }

    #[inline]
    pub fn raw_fd(&self) -> RawFd {
        self.file.as_raw_fd()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn card_name(&self) -> &str {
        &self.card_name
    }

    pub fn is_rally(&self) -> bool {
        self.profile.capabilities().vendor_name == "Logitech"
            || self.card_name.to_lowercase().contains("rally")
    }

    pub fn has_rally_xu(&self) -> bool {
        self.has_rally_xu
    }

    /// Query V4L2 capabilities
    pub fn query_capability(&self) -> Result<V4l2Capability> {
        let mut cap = V4l2Capability::default();
        unsafe {
            ioctl_call(self.raw_fd(), VIDIOC_QUERYCAP, &mut cap)
                .context("VIDIOC_QUERYCAP failed")?;
        }
        Ok(cap)
    }

    /// Get value of a V4L2 control
    pub fn get_ctrl(&self, cid: u32) -> Result<i32> {
        let mut ctrl = V4l2Control { id: cid, value: 0 };
        unsafe {
            ioctl_call(self.raw_fd(), VIDIOC_G_CTRL, &mut ctrl)
                .with_context(|| format!("Failed to get control 0x{:08x}", cid))?;
        }
        Ok(ctrl.value)
    }

    /// Set value of a V4L2 control
    pub fn set_ctrl(&self, cid: u32, val: i32) -> Result<()> {
        let mut ctrl = V4l2Control {
            id: cid,
            value: val,
        };
        unsafe {
            ioctl_call(self.raw_fd(), VIDIOC_S_CTRL, &mut ctrl)
                .with_context(|| format!("Failed to set control 0x{:08x} = {}", cid, val))?;
        }
        Ok(())
    }

    /// Query details of a V4L2 control
    pub fn query_ctrl(&self, cid: u32) -> Result<ControlInfo> {
        let mut q = V4l2QueryCtrl {
            id: cid,
            ..Default::default()
        };
        unsafe {
            ioctl_call(self.raw_fd(), VIDIOC_QUERYCTRL, &mut q)
                .with_context(|| format!("Failed to query control 0x{:08x}", cid))?;
        }

        let name = String::from_utf8_lossy(&q.name)
            .trim_matches(char::from(0))
            .to_string();

        let current_val = self.get_ctrl(cid).ok();

        Ok(ControlInfo {
            id: q.id,
            name,
            ctrl_type: q.ctrl_type,
            min: q.minimum,
            max: q.maximum,
            step: q.step,
            default_val: q.default_value,
            current_val,
        })
    }

    /// Query all supported controls on this camera
    pub fn query_all_ctrls(&self) -> Vec<ControlInfo> {
        let mut ctrls = Vec::new();
        let mut cid = V4L2_CTRL_FLAG_NEXT_CTRL;

        loop {
            let mut q = V4l2QueryCtrl {
                id: cid,
                ..Default::default()
            };
            let res = unsafe { ioctl_call(self.raw_fd(), VIDIOC_QUERYCTRL, &mut q) };
            if res.is_err() {
                break;
            }

            let name = String::from_utf8_lossy(&q.name)
                .trim_matches(char::from(0))
                .to_string();

            let current_val = self.get_ctrl(q.id).ok();

            ctrls.push(ControlInfo {
                id: q.id,
                name,
                ctrl_type: q.ctrl_type,
                min: q.minimum,
                max: q.maximum,
                step: q.step,
                default_val: q.default_value,
                current_val,
            });

            cid = q.id | V4L2_CTRL_FLAG_NEXT_CTRL;
        }

        ctrls
    }

    /// Check if Logitech UVC Extension Unit is supported
    fn check_xu_support(&self) -> bool {
        let mut len: u16 = 0;
        let mut query = UvcXuControlQuery {
            unit: LOGI_XU_PERIPHERAL_UNIT_ID,
            selector: XU_PERIPHERALCONTROL_PANTILT_MODE_CONTROL,
            query: UVC_GET_LEN,
            size: 2,
            data: (&mut len) as *mut u16 as *mut u8,
        };

        unsafe { ioctl_call(self.raw_fd(), UVCIOC_CTRL_QUERY, &mut query).is_ok() && len == 1 }
    }

    // -------------------------------------------------------------
    // High-Level PTZ Operations (Delegated to CameraProfile)
    // -------------------------------------------------------------

    /// Get Pan in degrees
    pub fn get_pan_deg(&self) -> Result<f32> {
        let raw = self.get_ctrl(V4L2_CID_PAN_ABSOLUTE)?;
        Ok(self.profile.limits().pan_raw_to_deg(raw))
    }

    /// Set Pan in degrees
    pub fn set_pan_deg(&self, deg: f32) -> Result<()> {
        let raw = self.profile.limits().deg_to_pan_raw(deg);
        self.set_ctrl(V4L2_CID_PAN_ABSOLUTE, raw)
    }

    /// Set Pan to target degrees (alias for set_pan_deg)
    #[inline]
    pub fn pan_to_deg(&self, deg: f32) -> Result<()> {
        self.set_pan_deg(deg)
    }

    /// Pan relatively by delta degrees
    pub fn pan_relative(&self, delta_deg: f32) -> Result<f32> {
        let current = self.get_pan_deg()?;
        let target = current + delta_deg;
        self.set_pan_deg(target)?;
        self.get_pan_deg()
    }

    /// Get Tilt in degrees
    pub fn get_tilt_deg(&self) -> Result<f32> {
        let raw = self.get_ctrl(V4L2_CID_TILT_ABSOLUTE)?;
        Ok(self.profile.limits().tilt_raw_to_deg(raw))
    }

    /// Set Tilt in degrees
    pub fn set_tilt_deg(&self, deg: f32) -> Result<()> {
        let raw = self.profile.limits().deg_to_tilt_raw(deg);
        self.set_ctrl(V4L2_CID_TILT_ABSOLUTE, raw)
    }

    /// Set Tilt to target degrees (alias for set_tilt_deg)
    #[inline]
    pub fn tilt_to_deg(&self, deg: f32) -> Result<()> {
        self.set_tilt_deg(deg)
    }

    /// Tilt relatively by delta degrees
    pub fn tilt_relative(&self, delta_deg: f32) -> Result<f32> {
        let current = self.get_tilt_deg()?;
        let target = current + delta_deg;
        self.set_tilt_deg(target)?;
        self.get_tilt_deg()
    }

    /// Move both Pan and Tilt simultaneously
    pub fn move_pan_tilt(&self, pan_deg: f32, tilt_deg: f32) -> Result<()> {
        self.set_pan_deg(pan_deg)?;
        self.set_tilt_deg(tilt_deg)?;
        Ok(())
    }

    /// Get Zoom multiplier
    pub fn get_zoom_x(&self) -> Result<f32> {
        let raw = self.get_ctrl(V4L2_CID_ZOOM_ABSOLUTE)?;
        Ok(self.profile.limits().zoom_raw_to_x(raw))
    }

    /// Set Zoom multiplier
    pub fn set_zoom_x(&self, x: f32) -> Result<()> {
        let raw = self.profile.limits().zoom_x_to_raw(x);
        self.set_ctrl(V4L2_CID_ZOOM_ABSOLUTE, raw)
    }

    /// Set Zoom multiplier (alias for set_zoom_x)
    #[inline]
    pub fn zoom_to(&self, x: f32) -> Result<()> {
        self.set_zoom_x(x)
    }

    /// Zoom relatively by delta multiplier
    pub fn zoom_relative(&self, delta_x: f32) -> Result<f32> {
        let current = self.get_zoom_x()?;
        let target = current + delta_x;
        self.set_zoom_x(target)?;
        self.get_zoom_x()
    }

    /// Set Continuous Pan speed (-1: left, 1: right, 0: stop)
    pub fn set_pan_speed(&self, speed: i32) -> Result<()> {
        let clamped = speed.clamp(-1, 1);
        self.set_ctrl(V4L2_CID_PAN_SPEED, clamped)
    }

    /// Set Continuous Tilt speed (-1: down, 1: up, 0: stop)
    pub fn set_tilt_speed(&self, speed: i32) -> Result<()> {
        let clamped = speed.clamp(-1, 1);
        self.set_ctrl(V4L2_CID_TILT_SPEED, clamped)
    }

    /// Stop all active movement motors
    pub fn stop_motion(&self) -> Result<()> {
        let _ = self.set_pan_speed(0);
        let _ = self.set_tilt_speed(0);
        Ok(())
    }

    /// Reset camera to Hardware Home Position (delegating to profile)
    pub fn home(&self) -> Result<()> {
        let _ = self.profile.home(self.raw_fd());
        let limits = self.profile.limits();
        let _ = self.set_ctrl(V4L2_CID_PAN_ABSOLUTE, 0);
        let _ = self.set_ctrl(V4L2_CID_TILT_ABSOLUTE, 0);
        let _ = self.set_ctrl(V4L2_CID_ZOOM_ABSOLUTE, limits.zoom_min);
        Ok(())
    }

    /// Reset camera to Hardware Home Position (backward-compatible alias)
    #[inline]
    pub fn goto_home(&self) -> Result<()> {
        self.home()
    }

    /// Recall a hardware EEPROM preset (1..8)
    pub fn recall_hardware_preset(&self, preset_num: u8) -> Result<()> {
        self.profile
            .recall_hardware_preset(self.raw_fd(), preset_num)
    }

    /// Save current position to a hardware EEPROM preset (1..8)
    pub fn save_hardware_preset(&self, preset_num: u8) -> Result<()> {
        self.profile.save_hardware_preset(self.raw_fd(), preset_num)
    }

    /// Move camera into privacy/sleep mode (e.g. downward park)
    pub fn sleep_privacy(&self) -> Result<()> {
        self.profile.sleep_privacy(self.raw_fd())
    }

    /// Wake camera from privacy mode
    pub fn wake_privacy(&self) -> Result<()> {
        self.profile.wake_privacy(self.raw_fd())
    }

    // -------------------------------------------------------------
    // Image and Sensor Controls
    // -------------------------------------------------------------

    /// Get current power line frequency setting (0: Disabled, 1: 50Hz, 2: 60Hz)
    pub fn get_power_line_frequency(&self) -> Result<i32> {
        self.get_ctrl(V4L2_CID_POWER_LINE_FREQUENCY)
    }

    /// Set power line frequency setting (anti-flicker: 0, 50, 60 Hz)
    pub fn set_power_line_frequency(&self, hz: u32) -> Result<()> {
        let val = match hz {
            0 => FLICKER_DISABLED,
            50 => FLICKER_50HZ,
            60 => FLICKER_60HZ,
            _ => bail!("Invalid power line frequency: {} (choose 0, 50, or 60)", hz),
        };
        self.set_ctrl(V4L2_CID_POWER_LINE_FREQUENCY, val)
    }

    /// Cycle through anti-flicker modes: Disabled -> 50Hz -> 60Hz -> Disabled
    pub fn cycle_power_line_frequency(&self) -> Result<i32> {
        let current = self.get_power_line_frequency().unwrap_or(0);
        let next = match current {
            FLICKER_DISABLED => FLICKER_50HZ,
            FLICKER_50HZ => FLICKER_60HZ,
            _ => FLICKER_DISABLED,
        };
        self.set_ctrl(V4L2_CID_POWER_LINE_FREQUENCY, next)?;
        Ok(next)
    }

    pub fn get_brightness(&self) -> Result<i32> {
        self.get_ctrl(V4L2_CID_BRIGHTNESS)
    }

    pub fn set_brightness(&self, val: i32) -> Result<()> {
        self.set_ctrl(V4L2_CID_BRIGHTNESS, val.clamp(0, 255))
    }

    pub fn get_contrast(&self) -> Result<i32> {
        self.get_ctrl(V4L2_CID_CONTRAST)
    }

    pub fn set_contrast(&self, val: i32) -> Result<()> {
        self.set_ctrl(V4L2_CID_CONTRAST, val.clamp(0, 255))
    }

    pub fn get_saturation(&self) -> Result<i32> {
        self.get_ctrl(V4L2_CID_SATURATION)
    }

    pub fn set_saturation(&self, val: i32) -> Result<()> {
        self.set_ctrl(V4L2_CID_SATURATION, val.clamp(0, 255))
    }

    pub fn get_sharpness(&self) -> Result<i32> {
        self.get_ctrl(V4L2_CID_SHARPNESS)
    }

    pub fn set_sharpness(&self, val: i32) -> Result<()> {
        self.set_ctrl(V4L2_CID_SHARPNESS, val.clamp(0, 255))
    }

    pub fn get_auto_exposure(&self) -> Result<i32> {
        self.get_ctrl(V4L2_CID_EXPOSURE_AUTO)
    }

    pub fn set_auto_exposure(&self, auto: bool) -> Result<()> {
        // 1: Manual Mode, 3: Aperture Priority (Auto)
        let val = if auto { 3 } else { 1 };
        self.set_ctrl(V4L2_CID_EXPOSURE_AUTO, val)
    }

    pub fn get_exposure_time(&self) -> Result<i32> {
        self.get_ctrl(V4L2_CID_EXPOSURE_ABSOLUTE)
    }

    pub fn set_exposure_time(&self, val: i32) -> Result<()> {
        self.set_ctrl(V4L2_CID_EXPOSURE_ABSOLUTE, val)
    }

    pub fn get_auto_focus(&self) -> Result<bool> {
        let val = self.get_ctrl(V4L2_CID_FOCUS_AUTO)?;
        Ok(val != 0)
    }

    pub fn set_auto_focus(&self, auto: bool) -> Result<()> {
        self.set_ctrl(V4L2_CID_FOCUS_AUTO, if auto { 1 } else { 0 })
    }

    /// Read comprehensive status
    pub fn get_status(&self) -> CameraStatus {
        let limits = self.profile.limits();
        let pan_raw = self.get_ctrl(V4L2_CID_PAN_ABSOLUTE).unwrap_or(0);
        let tilt_raw = self.get_ctrl(V4L2_CID_TILT_ABSOLUTE).unwrap_or(0);
        let zoom_raw = self
            .get_ctrl(V4L2_CID_ZOOM_ABSOLUTE)
            .unwrap_or(limits.zoom_min);

        CameraStatus {
            device_path: self.path.to_string_lossy().to_string(),
            card_name: self.card_name.clone(),
            pan_deg: limits.pan_raw_to_deg(pan_raw),
            pan_raw,
            tilt_deg: limits.tilt_raw_to_deg(tilt_raw),
            tilt_raw,
            zoom_x: limits.zoom_raw_to_x(zoom_raw),
            zoom_raw,
            brightness: self.get_brightness().ok(),
            contrast: self.get_contrast().ok(),
            saturation: self.get_saturation().ok(),
            sharpness: self.get_sharpness().ok(),
            power_line_frequency: self.get_power_line_frequency().ok(),
            auto_exposure: self.get_auto_exposure().ok(),
            exposure_time: self.get_exposure_time().ok(),
            auto_focus: self.get_auto_focus().ok(),
            white_balance_temp: self.get_ctrl(V4L2_CID_WHITE_BALANCE_TEMPERATURE).ok(),
            auto_white_balance: self
                .get_ctrl(V4L2_CID_AUTO_WHITE_BALANCE)
                .map(|v| v != 0)
                .ok(),
        }
    }
}
