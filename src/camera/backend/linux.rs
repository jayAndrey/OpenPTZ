#![allow(dead_code)]

use std::fs::{self, File, OpenOptions};
use std::os::raw::{c_int, c_ulong};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::camera::constants::*;
use crate::camera::types::*;

// V4L2 Control IDs
pub const V4L2_CID_BASE: u32 = 0x00980900;
pub const V4L2_CID_BRIGHTNESS: u32 = V4L2_CID_BASE;
pub const V4L2_CID_CONTRAST: u32 = V4L2_CID_BASE + 1;
pub const V4L2_CID_SATURATION: u32 = V4L2_CID_BASE + 2;
pub const V4L2_CID_HUE: u32 = V4L2_CID_BASE + 3;
pub const V4L2_CID_AUTO_WHITE_BALANCE: u32 = V4L2_CID_BASE + 12;
pub const V4L2_CID_POWER_LINE_FREQUENCY: u32 = V4L2_CID_BASE + 24;
pub const V4L2_CID_WHITE_BALANCE_TEMPERATURE: u32 = V4L2_CID_BASE + 26;
pub const V4L2_CID_SHARPNESS: u32 = V4L2_CID_BASE + 27;

pub const V4L2_CID_CAMERA_CLASS_BASE: u32 = 0x009a0900;
pub const V4L2_CID_EXPOSURE_AUTO: u32 = V4L2_CID_CAMERA_CLASS_BASE + 1;
pub const V4L2_CID_EXPOSURE_ABSOLUTE: u32 = V4L2_CID_CAMERA_CLASS_BASE + 2;
pub const V4L2_CID_PAN_RELATIVE: u32 = V4L2_CID_CAMERA_CLASS_BASE + 4;
pub const V4L2_CID_TILT_RELATIVE: u32 = V4L2_CID_CAMERA_CLASS_BASE + 5;
pub const V4L2_CID_PAN_ABSOLUTE: u32 = V4L2_CID_CAMERA_CLASS_BASE + 8;
pub const V4L2_CID_TILT_ABSOLUTE: u32 = V4L2_CID_CAMERA_CLASS_BASE + 9;
pub const V4L2_CID_FOCUS_AUTO: u32 = V4L2_CID_CAMERA_CLASS_BASE + 12;
pub const V4L2_CID_ZOOM_ABSOLUTE: u32 = V4L2_CID_CAMERA_CLASS_BASE + 13;
pub const V4L2_CID_PAN_SPEED: u32 = V4L2_CID_CAMERA_CLASS_BASE + 32;
pub const V4L2_CID_TILT_SPEED: u32 = V4L2_CID_CAMERA_CLASS_BASE + 33;

pub const LOGI_XU_PERIPHERAL_UNIT_ID: u8 = 11;
pub const XU_PERIPHERALCONTROL_PANTILT_MODE_CONTROL: u8 = 0x02;
pub const LOGI_XU_CMD_HOME: u8 = 3;

// IOCTL request codes
pub const VIDIOC_QUERYCAP: c_ulong = 0x80685600;
pub const VIDIOC_G_CTRL: c_ulong = 0xc008561b;
pub const VIDIOC_S_CTRL: c_ulong = 0xc008561c;
pub const VIDIOC_QUERYCTRL: c_ulong = 0xc0445624;
pub const UVCIOC_CTRL_QUERY: c_ulong = 0xc0107521;
pub const V4L2_CTRL_FLAG_NEXT_CTRL: u32 = 0x80000000;

pub const UVC_SET_CUR: u8 = 0x01;
pub const UVC_GET_LEN: u8 = 0x85;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct V4l2Capability {
    pub driver: [u8; 16],
    pub card: [u8; 32],
    pub bus_info: [u8; 32],
    pub version: u32,
    pub capabilities: u32,
    pub device_caps: u32,
    pub reserved: [u32; 3],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct V4l2Control {
    pub id: u32,
    pub value: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct V4l2QueryCtrl {
    pub id: u32,
    pub ctrl_type: u32,
    pub name: [u8; 32],
    pub minimum: i32,
    pub maximum: i32,
    pub step: i32,
    pub default_value: i32,
    pub flags: u32,
    pub reserved: [u32; 2],
}

#[repr(C)]
#[derive(Debug)]
pub struct UvcXuControlQuery {
    pub unit: u8,
    pub selector: u8,
    pub query: u8,
    pub size: u16,
    pub data: *mut u8,
}

#[allow(clippy::missing_safety_doc)]
pub unsafe fn ioctl_call<T>(fd: RawFd, request: c_ulong, arg: *mut T) -> std::io::Result<c_int> {
    let ret = libc::ioctl(fd, request, arg);
    if ret < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(ret)
    }
}

pub struct CameraDeviceImpl {
    file: File,
    path: PathBuf,
    card_name: String,
    has_rally_xu: bool,
}

impl CameraDeviceImpl {
    pub fn discover_devices() -> Vec<DeviceInfo> {
        let mut devices = Vec::new();
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

                        let is_rally = card.to_lowercase().contains("rally");
                        let supports_ptz = is_rally
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

    fn open_read_only(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
            .with_context(|| format!("Failed to open device {:?}", path))?;

        Ok(Self {
            file,
            path: path.to_path_buf(),
            card_name: String::new(),
            has_rally_xu: false,
        })
    }

    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path_buf)
            .with_context(|| format!("Failed to open camera device {:?}", path_buf))?;

        let mut dev = Self {
            file,
            path: path_buf,
            card_name: String::new(),
            has_rally_xu: false,
        };

        if let Ok(cap) = dev.query_capability() {
            dev.card_name = String::from_utf8_lossy(&cap.card)
                .trim_matches(char::from(0))
                .to_string();
        }

        dev.has_rally_xu = dev.check_xu_support();
        Ok(dev)
    }

    pub fn open_default() -> Result<Self> {
        let devs = Self::discover_devices();
        if devs.is_empty() {
            bail!("No V4L2 video devices found on system.");
        }

        if let Some(rally) = devs.iter().find(|d| d.is_rally) {
            return Self::open(&rally.path);
        }

        if let Some(ptz) = devs.iter().find(|d| d.supports_ptz) {
            return Self::open(&ptz.path);
        }

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

    pub fn has_rally_xu(&self) -> bool {
        self.has_rally_xu
    }

    pub fn query_capability(&self) -> Result<V4l2Capability> {
        let mut cap = V4l2Capability::default();
        unsafe {
            ioctl_call(self.raw_fd(), VIDIOC_QUERYCAP, &mut cap)
                .context("VIDIOC_QUERYCAP failed")?;
        }
        Ok(cap)
    }

    pub fn get_ctrl(&self, cid: u32) -> Result<i32> {
        let mut ctrl = V4l2Control { id: cid, value: 0 };
        unsafe {
            ioctl_call(self.raw_fd(), VIDIOC_G_CTRL, &mut ctrl)
                .with_context(|| format!("Failed to get control 0x{:08x}", cid))?;
        }
        Ok(ctrl.value)
    }

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

    pub fn get_pan_deg(&self) -> Result<f32> {
        let raw = self.get_ctrl(V4L2_CID_PAN_ABSOLUTE)?;
        Ok(raw_to_deg(raw))
    }

    pub fn set_pan_deg(&self, deg: f32) -> Result<()> {
        let clamped_deg = deg.clamp(-90.0, 90.0);
        let raw = deg_to_raw(clamped_deg).clamp(RALLY_PAN_MIN, RALLY_PAN_MAX);
        self.set_ctrl(V4L2_CID_PAN_ABSOLUTE, raw)
    }

    pub fn pan_relative(&self, delta_deg: f32) -> Result<f32> {
        let current = self.get_pan_deg()?;
        let target = current + delta_deg;
        self.set_pan_deg(target)?;
        self.get_pan_deg()
    }

    pub fn get_tilt_deg(&self) -> Result<f32> {
        let raw = self.get_ctrl(V4L2_CID_TILT_ABSOLUTE)?;
        Ok(raw_to_deg(raw))
    }

    pub fn set_tilt_deg(&self, deg: f32) -> Result<()> {
        let clamped_deg = deg.clamp(-90.0, 50.0);
        let raw = deg_to_raw(clamped_deg).clamp(RALLY_TILT_MIN, RALLY_TILT_MAX);
        self.set_ctrl(V4L2_CID_TILT_ABSOLUTE, raw)
    }

    pub fn tilt_relative(&self, delta_deg: f32) -> Result<f32> {
        let current = self.get_tilt_deg()?;
        let target = current + delta_deg;
        self.set_tilt_deg(target)?;
        self.get_tilt_deg()
    }

    pub fn move_pan_tilt(&self, pan_deg: f32, tilt_deg: f32) -> Result<()> {
        self.set_pan_deg(pan_deg)?;
        self.set_tilt_deg(tilt_deg)?;
        Ok(())
    }

    pub fn get_zoom_x(&self) -> Result<f32> {
        let raw = self.get_ctrl(V4L2_CID_ZOOM_ABSOLUTE)?;
        Ok(zoom_raw_to_x(raw))
    }

    pub fn set_zoom_x(&self, x: f32) -> Result<()> {
        let clamped_x = x.clamp(1.0, 15.0);
        let raw = zoom_x_to_raw(clamped_x).clamp(RALLY_ZOOM_MIN, RALLY_ZOOM_MAX);
        self.set_ctrl(V4L2_CID_ZOOM_ABSOLUTE, raw)
    }

    pub fn zoom_relative(&self, delta_x: f32) -> Result<f32> {
        let current = self.get_zoom_x()?;
        let target = current + delta_x;
        self.set_zoom_x(target)?;
        self.get_zoom_x()
    }

    pub fn set_pan_speed(&self, speed: i32) -> Result<()> {
        let clamped = speed.clamp(-1, 1);
        self.set_ctrl(V4L2_CID_PAN_SPEED, clamped)
    }

    pub fn set_tilt_speed(&self, speed: i32) -> Result<()> {
        let clamped = speed.clamp(-1, 1);
        self.set_ctrl(V4L2_CID_TILT_SPEED, clamped)
    }

    pub fn stop_motion(&self) -> Result<()> {
        let _ = self.set_pan_speed(0);
        let _ = self.set_tilt_speed(0);
        Ok(())
    }

    pub fn goto_home(&self) -> Result<()> {
        if self.has_rally_xu {
            let mut val: u8 = LOGI_XU_CMD_HOME;
            let mut query = UvcXuControlQuery {
                unit: LOGI_XU_PERIPHERAL_UNIT_ID,
                selector: XU_PERIPHERALCONTROL_PANTILT_MODE_CONTROL,
                query: UVC_SET_CUR,
                size: 1,
                data: &mut val as *mut u8,
            };

            unsafe {
                let _ = ioctl_call(self.raw_fd(), UVCIOC_CTRL_QUERY, &mut query);
            }
        }

        let _ = self.set_ctrl(V4L2_CID_PAN_ABSOLUTE, 0);
        let _ = self.set_ctrl(V4L2_CID_TILT_ABSOLUTE, 0);
        let _ = self.set_ctrl(V4L2_CID_ZOOM_ABSOLUTE, RALLY_ZOOM_MIN);

        Ok(())
    }

    pub fn get_power_line_frequency(&self) -> Result<i32> {
        self.get_ctrl(V4L2_CID_POWER_LINE_FREQUENCY)
    }

    pub fn set_power_line_frequency(&self, hz: u32) -> Result<()> {
        let val = match hz {
            0 => FLICKER_DISABLED,
            50 => FLICKER_50HZ,
            60 => FLICKER_60HZ,
            _ => bail!("Invalid power line frequency: {} (choose 0, 50, or 60)", hz),
        };
        self.set_ctrl(V4L2_CID_POWER_LINE_FREQUENCY, val)
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

    pub fn get_status(&self) -> CameraStatus {
        let pan_raw = self.get_ctrl(V4L2_CID_PAN_ABSOLUTE).unwrap_or(0);
        let tilt_raw = self.get_ctrl(V4L2_CID_TILT_ABSOLUTE).unwrap_or(0);
        let zoom_raw = self.get_ctrl(V4L2_CID_ZOOM_ABSOLUTE).unwrap_or(100);

        CameraStatus {
            device_path: self.path.to_string_lossy().to_string(),
            card_name: self.card_name.clone(),
            pan_deg: raw_to_deg(pan_raw),
            pan_raw,
            tilt_deg: raw_to_deg(tilt_raw),
            tilt_raw,
            zoom_x: zoom_raw_to_x(zoom_raw),
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
