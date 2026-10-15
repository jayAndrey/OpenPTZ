#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Result};
use windows::core::{Interface, GUID, PCWSTR, VARIANT};
use windows::Win32::Media::DirectShow::*;
use windows::Win32::Media::KernelStreaming::*;
use windows::Win32::System::Com::StructuredStorage::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Variant::*;

use crate::camera::constants::*;
use crate::camera::types::*;
use crate::cameras::CameraProfile;

// DirectShow Device Enumerator CLSIDs
const CLSID_SYSTEM_DEVICE_ENUM: GUID = GUID::from_u128(0x62be5d10_60eb_11d0_bd3b_00a0c911ce86);
const CLSID_VIDEO_INPUT_DEVICE_CATEGORY: GUID =
    GUID::from_u128(0x860bb310_5d01_11d0_bd3b_00a0c911ce86);

// VideoProcAmp Property for Anti-Flicker / Power Line Frequency
const VIDEO_PROC_AMP_POWERLINE_FREQUENCY: VideoProcAmpProperty = VideoProcAmpProperty(13);

pub struct CameraDeviceImpl {
    filter: IBaseFilter,
    cam_control: Option<IAMCameraControl>,
    proc_amp: Option<IAMVideoProcAmp>,
    ks_control: Option<IKsControl>,
    xu_peripheral_node_id: Option<u32>,
    path: PathBuf,
    card_name: String,
    has_rally_xu: bool,
    pub profile: Arc<dyn CameraProfile>,
    // Software-tracked state for when camera properties are write-only or relative
    cached_pan_deg: Mutex<f32>,
    cached_tilt_deg: Mutex<f32>,
    cached_zoom_x: Mutex<f32>,
}

unsafe impl Send for CameraDeviceImpl {}
unsafe impl Sync for CameraDeviceImpl {}

fn init_com() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

fn read_variant_string(prop_bag: &IPropertyBag, prop_name: &str) -> Option<String> {
    let wide_name: Vec<u16> = prop_name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut var = VARIANT::default();
    unsafe {
        if prop_bag
            .Read(PCWSTR(wide_name.as_ptr()), &mut var, None)
            .is_ok()
        {
            let mut buf = [0u16; 512];
            let res = if VariantToString(&var, &mut buf).is_ok() {
                let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                Some(String::from_utf16_lossy(&buf[..len]))
            } else {
                None
            };
            let _ = VariantClear(&mut var);
            return res;
        }
    }
    None
}

impl CameraDeviceImpl {
    pub fn discover_devices() -> Vec<DeviceInfo> {
        init_com();
        let mut devices = Vec::new();

        unsafe {
            let dev_enum: ICreateDevEnum =
                match CoCreateInstance(&CLSID_SYSTEM_DEVICE_ENUM, None, CLSCTX_INPROC_SERVER) {
                    Ok(e) => e,
                    Err(_) => return devices,
                };

            let mut enum_moniker: Option<IEnumMoniker> = None;
            if dev_enum
                .CreateClassEnumerator(&CLSID_VIDEO_INPUT_DEVICE_CATEGORY, &mut enum_moniker, 0)
                .is_err()
            {
                return devices;
            }

            let enum_moniker = match enum_moniker {
                Some(e) => e,
                None => return devices,
            };

            let mut monikers: [Option<IMoniker>; 1] = [None];
            let mut index = 0;

            while enum_moniker.Next(&mut monikers, None).is_ok() {
                let moniker = match monikers[0].take() {
                    Some(m) => m,
                    None => break,
                };

                let card_name = if let Ok(prop_bag) = moniker
                    .BindToStorage::<Option<&IBindCtx>, Option<&IMoniker>, IPropertyBag>(None, None)
                {
                    read_variant_string(&prop_bag, "FriendlyName")
                        .unwrap_or_else(|| format!("Camera {}", index))
                } else {
                    format!("Camera {}", index)
                };

                let dev_path_str = if let Ok(prop_bag) = moniker
                    .BindToStorage::<Option<&IBindCtx>, Option<&IMoniker>, IPropertyBag>(None, None)
                {
                    read_variant_string(&prop_bag, "DevicePath")
                        .unwrap_or_else(|| format!("video{}", index))
                } else {
                    format!("video{}", index)
                };

                let is_rally = card_name.to_lowercase().contains("rally")
                    || dev_path_str.to_lowercase().contains("rally");

                // Check PTZ support by testing IAMCameraControl
                let mut supports_ptz = is_rally;
                if let Ok(filter) = moniker
                    .BindToObject::<Option<&IBindCtx>, Option<&IMoniker>, IBaseFilter>(None, None)
                {
                    if let Ok(cam_ctrl) = filter.cast::<IAMCameraControl>() {
                        let mut min = 0;
                        let mut max = 0;
                        let mut step = 0;
                        let mut def = 0;
                        let mut flags = 0;
                        if cam_ctrl
                            .GetRange(
                                CameraControl_Pan.0,
                                &mut min,
                                &mut max,
                                &mut step,
                                &mut def,
                                &mut flags,
                            )
                            .is_ok()
                        {
                            supports_ptz = true;
                        }
                    }
                }

                devices.push(DeviceInfo {
                    path: PathBuf::from(&dev_path_str),
                    card_name,
                    driver: "DirectShow".to_string(),
                    bus_info: format!("Device {}", index),
                    is_rally,
                    supports_ptz,
                });

                index += 1;
            }
        }

        devices
    }

    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        init_com();
        let path_ref = path.as_ref();
        let path_str = path_ref.to_string_lossy().to_string();

        unsafe {
            let dev_enum: ICreateDevEnum =
                CoCreateInstance(&CLSID_SYSTEM_DEVICE_ENUM, None, CLSCTX_INPROC_SERVER)
                    .context("Failed to create SystemDeviceEnum")?;

            let mut enum_moniker: Option<IEnumMoniker> = None;
            dev_enum
                .CreateClassEnumerator(&CLSID_VIDEO_INPUT_DEVICE_CATEGORY, &mut enum_moniker, 0)
                .context("Failed to enumerate VideoInputDeviceCategory")?;

            let enum_moniker = enum_moniker.context("No video input devices found on system")?;
            let mut monikers: [Option<IMoniker>; 1] = [None];
            let mut index = 0;
            let mut selected_moniker = None;
            let mut selected_name = String::new();

            while enum_moniker.Next(&mut monikers, None).is_ok() {
                let moniker = match monikers[0].take() {
                    Some(m) => m,
                    None => break,
                };

                let name = if let Ok(prop_bag) = moniker
                    .BindToStorage::<Option<&IBindCtx>, Option<&IMoniker>, IPropertyBag>(None, None)
                {
                    read_variant_string(&prop_bag, "FriendlyName")
                        .unwrap_or_else(|| format!("Camera {}", index))
                } else {
                    format!("Camera {}", index)
                };

                let dev_path = if let Ok(prop_bag) = moniker
                    .BindToStorage::<Option<&IBindCtx>, Option<&IMoniker>, IPropertyBag>(None, None)
                {
                    read_variant_string(&prop_bag, "DevicePath")
                        .unwrap_or_else(|| format!("video{}", index))
                } else {
                    format!("video{}", index)
                };

                // Match by exact path, index, or name substring
                if path_str == dev_path
                    || path_str == index.to_string()
                    || path_str == format!("video{}", index)
                    || name.to_lowercase().contains(&path_str.to_lowercase())
                {
                    selected_moniker = Some(moniker);
                    selected_name = name;
                    break;
                }

                index += 1;
            }

            let moniker = selected_moniker
                .with_context(|| format!("Camera device '{:?}' not found", path_ref))?;
            let filter: IBaseFilter = moniker
                .BindToObject(None, None)
                .context("Failed to bind camera moniker to IBaseFilter")?;

            let cam_control = filter.cast::<IAMCameraControl>().ok();
            let proc_amp = filter.cast::<IAMVideoProcAmp>().ok();
            let ks_control: Option<IKsControl> = moniker
                .BindToObject(None, None)
                .ok()
                .or_else(|| filter.cast::<IKsControl>().ok());

            let mut xu_peripheral_node_id = None;
            if let Some(ref ks) = ks_control {
                if let Ok(topo) = ks.cast::<IKsTopologyInfo>() {
                    if let Ok(num_nodes) = topo.NumNodes() {
                        const LOGITECH_XU_PERIPHERAL: GUID =
                            GUID::from_u128(0xffe52d21_8030_4e2c_82d9_f587d00540bd);
                        for node_id in 0..num_nodes {
                            let mut ext_prop = KSP_NODE::default();
                            ext_prop.Property.Anonymous.Anonymous.Set = LOGITECH_XU_PERIPHERAL;
                            ext_prop.Property.Anonymous.Anonymous.Id = 0;
                            ext_prop.Property.Anonymous.Anonymous.Flags =
                                KSPROPERTY_TYPE_SETSUPPORT | KSPROPERTY_TYPE_TOPOLOGY;
                            ext_prop.NodeId = node_id;
                            let mut bytes_returned = 0;
                            let hr = ks.KsProperty(
                                &ext_prop as *const _ as *const _,
                                std::mem::size_of::<KSP_NODE>() as u32,
                                std::ptr::null_mut(),
                                0,
                                &mut bytes_returned,
                            );
                            if hr.is_ok() {
                                xu_peripheral_node_id = Some(node_id);
                                break;
                            }
                        }
                    }
                }
            }

            let profile = crate::cameras::detect_profile(&selected_name, None);
            let has_rally_xu = xu_peripheral_node_id.is_some()
                || profile.capabilities().vendor_name == "Logitech"
                || selected_name.to_lowercase().contains("rally");

            Ok(Self {
                filter,
                cam_control,
                proc_amp,
                ks_control,
                xu_peripheral_node_id,
                path: path_ref.to_path_buf(),
                card_name: selected_name,
                has_rally_xu,
                profile,
                cached_pan_deg: Mutex::new(0.0),
                cached_tilt_deg: Mutex::new(0.0),
                cached_zoom_x: Mutex::new(1.0),
            })
        }
    }

    pub fn open_default() -> Result<Self> {
        let devices = Self::discover_devices();
        if devices.is_empty() {
            bail!("No video capture devices found on Windows system.");
        }

        // 1. Look for Logitech Rally first
        if let Some(rally) = devices.iter().find(|d| d.is_rally) {
            return Self::open(&rally.path);
        }

        // 2. Look for any camera supporting PTZ
        if let Some(ptz) = devices.iter().find(|d| d.supports_ptz) {
            return Self::open(&ptz.path);
        }

        // 3. Fallback to first device
        Self::open(&devices[0].path)
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

    pub fn xu_set_peripheral_property(&self, property_id: u32, value: u32) -> Result<()> {
        let (ks, node_id) = match (&self.ks_control, self.xu_peripheral_node_id) {
            (Some(ks), Some(node_id)) => (ks, node_id),
            _ => bail!("Logitech XU Peripheral control is not available on this device"),
        };

        const LOGITECH_XU_PERIPHERAL: GUID =
            GUID::from_u128(0xffe52d21_8030_4e2c_82d9_f587d00540bd);

        let mut ext_prop = KSP_NODE::default();
        ext_prop.Property.Anonymous.Anonymous.Set = LOGITECH_XU_PERIPHERAL;
        ext_prop.Property.Anonymous.Anonymous.Id = property_id;
        ext_prop.Property.Anonymous.Anonymous.Flags =
            KSPROPERTY_TYPE_SET | KSPROPERTY_TYPE_TOPOLOGY;
        ext_prop.NodeId = node_id;

        let mut val = value;
        let mut bytes_returned = 0;
        unsafe {
            ks.KsProperty(
                &ext_prop as *const _ as *const _,
                std::mem::size_of::<KSP_NODE>() as u32,
                &mut val as *mut _ as *mut _,
                std::mem::size_of::<u32>() as u32,
                &mut bytes_returned,
            )
            .context("Failed to send Logitech XU property")?;
        }
        Ok(())
    }

    pub fn query_all_ctrls(&self) -> Vec<ControlInfo> {
        let mut ctrls = Vec::new();
        if let Some(ref cc) = self.cam_control {
            let ptz_props = [
                (CameraControl_Pan.0, "Pan"),
                (CameraControl_Tilt.0, "Tilt"),
                (CameraControl_Zoom.0, "Zoom"),
                (CameraControl_Exposure.0, "Exposure"),
                (CameraControl_Focus.0, "Focus"),
            ];
            for (prop_id, name) in ptz_props {
                let mut min = 0;
                let mut max = 0;
                let mut step = 0;
                let mut def = 0;
                let mut flags = 0;
                unsafe {
                    if cc
                        .GetRange(prop_id, &mut min, &mut max, &mut step, &mut def, &mut flags)
                        .is_ok()
                    {
                        let mut cur = 0;
                        let mut cur_flags = 0;
                        let current_val = if cc.Get(prop_id, &mut cur, &mut cur_flags).is_ok() {
                            Some(cur)
                        } else {
                            None
                        };
                        ctrls.push(ControlInfo {
                            id: prop_id as u32,
                            name: name.to_string(),
                            ctrl_type: 1,
                            min,
                            max,
                            step,
                            default_val: def,
                            current_val,
                        });
                    }
                }
            }
        }
        ctrls
    }

    // -------------------------------------------------------------
    // High-Level PTZ Operations (Delegated to Profile limits)
    // -------------------------------------------------------------

    pub fn get_pan_deg(&self) -> Result<f32> {
        if self.xu_peripheral_node_id.is_some() || self.is_rally() {
            return Ok(*self.cached_pan_deg.lock().unwrap());
        }
        if let Some(ref cc) = self.cam_control {
            let mut min = 0;
            let mut max = 0;
            let mut step = 0;
            let mut def = 0;
            let mut flags = 0;
            unsafe {
                if cc
                    .GetRange(
                        CameraControl_Pan.0,
                        &mut min,
                        &mut max,
                        &mut step,
                        &mut def,
                        &mut flags,
                    )
                    .is_ok()
                {
                    let mut val = 0;
                    let mut cur_flags = 0;
                    if cc
                        .Get(CameraControl_Pan.0, &mut val, &mut cur_flags)
                        .is_ok()
                    {
                        let deg = if max.abs() > 360 {
                            val as f32 / UNITS_PER_DEGREE
                        } else {
                            val as f32
                        };
                        *self.cached_pan_deg.lock().unwrap() = deg;
                        return Ok(deg);
                    }
                }
            }
        }
        Ok(*self.cached_pan_deg.lock().unwrap())
    }

    pub fn set_pan_deg(&self, deg: f32) -> Result<()> {
        let limits = self.profile.limits();
        let min_deg = limits.pan_raw_to_deg(limits.pan_min);
        let max_deg = limits.pan_raw_to_deg(limits.pan_max);
        let clamped_deg = deg.clamp(min_deg, max_deg);

        let current = *self.cached_pan_deg.lock().unwrap();
        let delta = clamped_deg - current;
        if delta.abs() > 0.2 {
            self.pan_relative(delta)?;
        }
        *self.cached_pan_deg.lock().unwrap() = clamped_deg;
        Ok(())
    }

    #[inline]
    pub fn pan_to_deg(&self, deg: f32) -> Result<()> {
        self.set_pan_deg(deg)
    }

    pub fn pan_relative(&self, delta_deg: f32) -> Result<f32> {
        let limits = self.profile.limits();
        let min_deg = limits.pan_raw_to_deg(limits.pan_min);
        let max_deg = limits.pan_raw_to_deg(limits.pan_max);

        let current = *self.cached_pan_deg.lock().unwrap();
        let target = (current + delta_deg).clamp(min_deg, max_deg);
        let actual_delta = target - current;

        let dir: i8 = if actual_delta > 0.0 {
            1
        } else if actual_delta < 0.0 {
            -1
        } else {
            0
        };
        if dir != 0 {
            let duration_ms = (actual_delta.abs() * 35.0).clamp(60.0, 1500.0) as u64;
            if self.xu_peripheral_node_id.is_some() {
                let dw_val = (dir as u8 as u32) << 8;
                let _ = self.xu_set_peripheral_property(1, dw_val);
                std::thread::sleep(std::time::Duration::from_millis(duration_ms));
                let _ = self.xu_set_peripheral_property(1, 0);
            } else if let Some(ref cc) = self.cam_control {
                unsafe {
                    let _ = cc.Set(10, dir as i32, 0);
                    let _ = cc.Set(
                        CameraControl_Pan.0,
                        target as i32,
                        CameraControl_Flags_Manual.0,
                    );
                    std::thread::sleep(std::time::Duration::from_millis(duration_ms));
                    let _ = cc.Set(10, 0, 0);
                }
            }
        }
        *self.cached_pan_deg.lock().unwrap() = target;
        Ok(target)
    }

    pub fn get_tilt_deg(&self) -> Result<f32> {
        if self.xu_peripheral_node_id.is_some() || self.is_rally() {
            return Ok(*self.cached_tilt_deg.lock().unwrap());
        }
        if let Some(ref cc) = self.cam_control {
            let mut min = 0;
            let mut max = 0;
            let mut step = 0;
            let mut def = 0;
            let mut flags = 0;
            unsafe {
                if cc
                    .GetRange(
                        CameraControl_Tilt.0,
                        &mut min,
                        &mut max,
                        &mut step,
                        &mut def,
                        &mut flags,
                    )
                    .is_ok()
                {
                    let mut val = 0;
                    let mut cur_flags = 0;
                    if cc
                        .Get(CameraControl_Tilt.0, &mut val, &mut cur_flags)
                        .is_ok()
                    {
                        let deg = if max.abs() > 360 {
                            val as f32 / UNITS_PER_DEGREE
                        } else {
                            val as f32
                        };
                        *self.cached_tilt_deg.lock().unwrap() = deg;
                        return Ok(deg);
                    }
                }
            }
        }
        Ok(*self.cached_tilt_deg.lock().unwrap())
    }

    pub fn set_tilt_deg(&self, deg: f32) -> Result<()> {
        let limits = self.profile.limits();
        let min_deg = limits.tilt_raw_to_deg(limits.tilt_min);
        let max_deg = limits.tilt_raw_to_deg(limits.tilt_max);
        let clamped_deg = deg.clamp(min_deg, max_deg);

        let current = *self.cached_tilt_deg.lock().unwrap();
        let delta = clamped_deg - current;
        if delta.abs() > 0.2 {
            self.tilt_relative(delta)?;
        }
        *self.cached_tilt_deg.lock().unwrap() = clamped_deg;
        Ok(())
    }

    #[inline]
    pub fn tilt_to_deg(&self, deg: f32) -> Result<()> {
        self.set_tilt_deg(deg)
    }

    pub fn tilt_relative(&self, delta_deg: f32) -> Result<f32> {
        let limits = self.profile.limits();
        let min_deg = limits.tilt_raw_to_deg(limits.tilt_min);
        let max_deg = limits.tilt_raw_to_deg(limits.tilt_max);

        let current = *self.cached_tilt_deg.lock().unwrap();
        let target = (current + delta_deg).clamp(min_deg, max_deg);
        let actual_delta = target - current;

        let dir: i8 = if actual_delta > 0.0 {
            -1
        } else if actual_delta < 0.0 {
            1
        } else {
            0
        };
        if dir != 0 {
            let duration_ms = (actual_delta.abs() * 35.0).clamp(60.0, 1500.0) as u64;
            if self.xu_peripheral_node_id.is_some() {
                let dw_val = (dir as u8 as u32) << 24;
                let _ = self.xu_set_peripheral_property(1, dw_val);
                std::thread::sleep(std::time::Duration::from_millis(duration_ms));
                let _ = self.xu_set_peripheral_property(1, 0);
            } else if let Some(ref cc) = self.cam_control {
                unsafe {
                    let _ = cc.Set(11, dir as i32, 0);
                    let _ = cc.Set(
                        CameraControl_Tilt.0,
                        target as i32,
                        CameraControl_Flags_Manual.0,
                    );
                    std::thread::sleep(std::time::Duration::from_millis(duration_ms));
                    let _ = cc.Set(11, 0, 0);
                }
            }
        }
        *self.cached_tilt_deg.lock().unwrap() = target;
        Ok(target)
    }

    pub fn move_pan_tilt(&self, pan_deg: f32, tilt_deg: f32) -> Result<()> {
        self.set_pan_deg(pan_deg)?;
        self.set_tilt_deg(tilt_deg)?;
        Ok(())
    }

    pub fn get_zoom_x(&self) -> Result<f32> {
        if self.xu_peripheral_node_id.is_some() || self.is_rally() {
            return Ok(*self.cached_zoom_x.lock().unwrap());
        }
        if let Some(ref cc) = self.cam_control {
            let mut min = 0;
            let mut max = 0;
            let mut step = 0;
            let mut def = 0;
            let mut flags = 0;
            unsafe {
                if cc
                    .GetRange(
                        CameraControl_Zoom.0,
                        &mut min,
                        &mut max,
                        &mut step,
                        &mut def,
                        &mut flags,
                    )
                    .is_ok()
                {
                    let mut val = 0;
                    let mut cur_flags = 0;
                    if cc
                        .Get(CameraControl_Zoom.0, &mut val, &mut cur_flags)
                        .is_ok()
                    {
                        let z = if max >= 100 {
                            val as f32 / 100.0
                        } else if max > min {
                            1.0 + (val - min) as f32 / (max - min) as f32 * 14.0
                        } else {
                            val as f32
                        };
                        *self.cached_zoom_x.lock().unwrap() = z;
                        return Ok(z);
                    }
                }
            }
        }
        Ok(*self.cached_zoom_x.lock().unwrap())
    }

    pub fn set_zoom_x(&self, x: f32) -> Result<()> {
        let limits = self.profile.limits();
        let min_x = limits.zoom_raw_to_x(limits.zoom_min);
        let max_x = limits.zoom_raw_to_x(limits.zoom_max);
        let clamped_x = x.clamp(min_x, max_x);
        *self.cached_zoom_x.lock().unwrap() = clamped_x;

        if let Some(ref cc) = self.cam_control {
            let mut min = 0;
            let mut max = 0;
            let mut step = 0;
            let mut def = 0;
            let mut flags = 0;
            unsafe {
                if cc
                    .GetRange(
                        CameraControl_Zoom.0,
                        &mut min,
                        &mut max,
                        &mut step,
                        &mut def,
                        &mut flags,
                    )
                    .is_ok()
                {
                    let raw = if max >= 100 {
                        limits.zoom_x_to_raw(clamped_x)
                    } else {
                        min + ((clamped_x - 1.0) / 14.0 * (max - min) as f32).round() as i32
                    }
                    .clamp(min, max);
                    let _ = cc.Set(CameraControl_Zoom.0, raw, CameraControl_Flags_Manual.0);
                }
            }
        }
        Ok(())
    }

    #[inline]
    pub fn zoom_to(&self, x: f32) -> Result<()> {
        self.set_zoom_x(x)
    }

    pub fn zoom_relative(&self, delta_x: f32) -> Result<f32> {
        let current = self.get_zoom_x()?;
        let target = current + delta_x;
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
        if self.xu_peripheral_node_id.is_some() {
            let _ = self.xu_set_peripheral_property(2, 3);
            std::thread::sleep(std::time::Duration::from_millis(1500));
        } else {
            self.set_pan_deg(0.0)?;
            self.set_tilt_deg(0.0)?;
        }
        *self.cached_pan_deg.lock().unwrap() = 0.0;
        *self.cached_tilt_deg.lock().unwrap() = 0.0;
        self.set_zoom_x(1.0)?;
        Ok(())
    }

    #[inline]
    pub fn goto_home(&self) -> Result<()> {
        self.home()
    }

    pub fn recall_hardware_preset(&self, preset_num: u8) -> Result<()> {
        if (1..=8).contains(&preset_num) && self.xu_peripheral_node_id.is_some() {
            self.xu_set_peripheral_property(2, (preset_num as u32 - 1) + 12)?;
            return Ok(());
        }
        anyhow::bail!("Hardware EEPROM presets are not directly accessible on this camera")
    }

    pub fn save_hardware_preset(&self, preset_num: u8) -> Result<()> {
        if (1..=8).contains(&preset_num) && self.xu_peripheral_node_id.is_some() {
            self.xu_set_peripheral_property(2, (preset_num as u32 - 1) + 4)?;
            return Ok(());
        }
        anyhow::bail!("Hardware EEPROM presets are not directly accessible on this camera")
    }

    pub fn sleep_privacy(&self) -> Result<()> {
        self.set_tilt_deg(-90.0)?;
        Ok(())
    }

    pub fn wake_privacy(&self) -> Result<()> {
        self.home()
    }

    pub fn get_power_line_frequency(&self) -> Result<i32> {
        if let Some(ref pa) = self.proc_amp {
            let mut val = 0;
            let mut flags = 0;
            unsafe {
                if pa
                    .Get(VIDEO_PROC_AMP_POWERLINE_FREQUENCY.0, &mut val, &mut flags)
                    .is_ok()
                {
                    return Ok(val);
                }
            }
        }
        Ok(0)
    }

    pub fn set_power_line_frequency(&self, hz: u32) -> Result<()> {
        let val = match hz {
            0 => FLICKER_DISABLED,
            50 => FLICKER_50HZ,
            60 => FLICKER_60HZ,
            _ => bail!("Invalid power line frequency: {} (choose 0, 50, or 60)", hz),
        };
        if let Some(ref pa) = self.proc_amp {
            unsafe {
                let _ = pa.Set(
                    VIDEO_PROC_AMP_POWERLINE_FREQUENCY.0,
                    val,
                    VideoProcAmp_Flags_Manual.0,
                );
            }
        }
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
        if let Some(ref pa) = self.proc_amp {
            let mut val = 0;
            let mut flags = 0;
            unsafe {
                if pa
                    .Get(VideoProcAmp_Brightness.0, &mut val, &mut flags)
                    .is_ok()
                {
                    return Ok(val);
                }
            }
        }
        bail!("Brightness not supported")
    }

    pub fn set_brightness(&self, val: i32) -> Result<()> {
        if let Some(ref pa) = self.proc_amp {
            unsafe {
                let _ = pa.Set(
                    VideoProcAmp_Brightness.0,
                    val.clamp(0, 255),
                    VideoProcAmp_Flags_Manual.0,
                );
            }
        }
        Ok(())
    }

    pub fn get_contrast(&self) -> Result<i32> {
        if let Some(ref pa) = self.proc_amp {
            let mut val = 0;
            let mut flags = 0;
            unsafe {
                if pa
                    .Get(VideoProcAmp_Contrast.0, &mut val, &mut flags)
                    .is_ok()
                {
                    return Ok(val);
                }
            }
        }
        bail!("Contrast not supported")
    }

    pub fn set_contrast(&self, val: i32) -> Result<()> {
        if let Some(ref pa) = self.proc_amp {
            unsafe {
                let _ = pa.Set(
                    VideoProcAmp_Contrast.0,
                    val.clamp(0, 255),
                    VideoProcAmp_Flags_Manual.0,
                );
            }
        }
        Ok(())
    }

    pub fn get_saturation(&self) -> Result<i32> {
        if let Some(ref pa) = self.proc_amp {
            let mut val = 0;
            let mut flags = 0;
            unsafe {
                if pa
                    .Get(VideoProcAmp_Saturation.0, &mut val, &mut flags)
                    .is_ok()
                {
                    return Ok(val);
                }
            }
        }
        bail!("Saturation not supported")
    }

    pub fn set_saturation(&self, val: i32) -> Result<()> {
        if let Some(ref pa) = self.proc_amp {
            unsafe {
                let _ = pa.Set(
                    VideoProcAmp_Saturation.0,
                    val.clamp(0, 255),
                    VideoProcAmp_Flags_Manual.0,
                );
            }
        }
        Ok(())
    }

    pub fn get_sharpness(&self) -> Result<i32> {
        if let Some(ref pa) = self.proc_amp {
            let mut val = 0;
            let mut flags = 0;
            unsafe {
                if pa
                    .Get(VideoProcAmp_Sharpness.0, &mut val, &mut flags)
                    .is_ok()
                {
                    return Ok(val);
                }
            }
        }
        bail!("Sharpness not supported")
    }

    pub fn set_sharpness(&self, val: i32) -> Result<()> {
        if let Some(ref pa) = self.proc_amp {
            unsafe {
                let _ = pa.Set(
                    VideoProcAmp_Sharpness.0,
                    val.clamp(0, 255),
                    VideoProcAmp_Flags_Manual.0,
                );
            }
        }
        Ok(())
    }

    pub fn get_auto_exposure(&self) -> Result<i32> {
        if let Some(ref cc) = self.cam_control {
            let mut val = 0;
            let mut flags = 0;
            unsafe {
                if cc
                    .Get(CameraControl_Exposure.0, &mut val, &mut flags)
                    .is_ok()
                {
                    return Ok(if flags == CameraControl_Flags_Auto.0 {
                        3
                    } else {
                        1
                    });
                }
            }
        }
        Ok(1)
    }

    pub fn set_auto_exposure(&self, auto: bool) -> Result<()> {
        if let Some(ref cc) = self.cam_control {
            let flags = if auto {
                CameraControl_Flags_Auto.0
            } else {
                CameraControl_Flags_Manual.0
            };
            unsafe {
                let _ = cc.Set(CameraControl_Exposure.0, 0, flags);
            }
        }
        Ok(())
    }

    pub fn get_exposure_time(&self) -> Result<i32> {
        if let Some(ref cc) = self.cam_control {
            let mut val = 0;
            let mut flags = 0;
            unsafe {
                if cc
                    .Get(CameraControl_Exposure.0, &mut val, &mut flags)
                    .is_ok()
                {
                    return Ok(val);
                }
            }
        }
        bail!("Exposure time not supported")
    }

    pub fn set_exposure_time(&self, val: i32) -> Result<()> {
        if let Some(ref cc) = self.cam_control {
            unsafe {
                let _ = cc.Set(CameraControl_Exposure.0, val, CameraControl_Flags_Manual.0);
            }
        }
        Ok(())
    }

    pub fn get_auto_focus(&self) -> Result<bool> {
        if let Some(ref cc) = self.cam_control {
            let mut val = 0;
            let mut flags = 0;
            unsafe {
                if cc.Get(CameraControl_Focus.0, &mut val, &mut flags).is_ok() {
                    return Ok(flags == CameraControl_Flags_Auto.0);
                }
            }
        }
        Ok(false)
    }

    pub fn set_auto_focus(&self, auto: bool) -> Result<()> {
        if let Some(ref cc) = self.cam_control {
            let flags = if auto {
                CameraControl_Flags_Auto.0
            } else {
                CameraControl_Flags_Manual.0
            };
            unsafe {
                let _ = cc.Set(CameraControl_Focus.0, 0, flags);
            }
        }
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
