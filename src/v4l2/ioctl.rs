#![allow(dead_code)]

use std::os::raw::{c_int, c_ulong};
use std::os::unix::io::RawFd;

// IOCTL request codes
pub const VIDIOC_QUERYCAP: c_ulong = 0x80685600;
pub const VIDIOC_G_CTRL: c_ulong = 0xc008561b;
pub const VIDIOC_S_CTRL: c_ulong = 0xc008561c;
pub const VIDIOC_QUERYCTRL: c_ulong = 0xc0445624;
pub const UVCIOC_CTRL_QUERY: c_ulong = 0xc0107521;

// V4L2 Control flags
pub const V4L2_CTRL_FLAG_DISABLED: u32 = 0x0001;
pub const V4L2_CTRL_FLAG_GRABBED: u32 = 0x0002;
pub const V4L2_CTRL_FLAG_READ_ONLY: u32 = 0x0004;
pub const V4L2_CTRL_FLAG_UPDATE: u32 = 0x0008;
pub const V4L2_CTRL_FLAG_INACTIVE: u32 = 0x0010;
pub const V4L2_CTRL_FLAG_SLIDER: u32 = 0x0020;
pub const V4L2_CTRL_FLAG_WRITE_ONLY: u32 = 0x0040;
pub const V4L2_CTRL_FLAG_VOLATILE: u32 = 0x0080;
pub const V4L2_CTRL_FLAG_HAS_PAYLOAD: u32 = 0x0100;
pub const V4L2_CTRL_FLAG_EXECUTE_ON_WRITE: u32 = 0x0200;
pub const V4L2_CTRL_FLAG_MODIFY_LAYOUT: u32 = 0x0400;
pub const V4L2_CTRL_FLAG_NEXT_CTRL: u32 = 0x80000000;
pub const V4L2_CTRL_FLAG_NEXT_COMPOUND: u32 = 0x40000000;

// UVC Request codes (UVC spec / linux/usb/video.h)
pub const UVC_SET_CUR: u8 = 0x01;
pub const UVC_GET_CUR: u8 = 0x81;
pub const UVC_GET_MIN: u8 = 0x82;
pub const UVC_GET_MAX: u8 = 0x83;
pub const UVC_GET_RES: u8 = 0x84;
pub const UVC_GET_LEN: u8 = 0x85;
pub const UVC_GET_INFO: u8 = 0x86;
pub const UVC_GET_DEF: u8 = 0x87;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct V4l2Capability {
    pub driver: [u8; 16],
    pub card: [u8; 32],
    pub bus_info: [u8; 32],
    pub version: u32,
    pub capabilities: u32,
    pub device_caps: u32,
    pub reserved: [u32; 3],
}

impl Default for V4l2Capability {
    fn default() -> Self {
        Self {
            driver: [0; 16],
            card: [0; 32],
            bus_info: [0; 32],
            version: 0,
            capabilities: 0,
            device_caps: 0,
            reserved: [0; 3],
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct V4l2Control {
    pub id: u32,
    pub value: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
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

impl Default for V4l2QueryCtrl {
    fn default() -> Self {
        Self {
            id: 0,
            ctrl_type: 0,
            name: [0; 32],
            minimum: 0,
            maximum: 0,
            step: 0,
            default_value: 0,
            flags: 0,
            reserved: [0; 2],
        }
    }
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

/// Perform an ioctl syscall safely
pub unsafe fn ioctl_call<T>(fd: RawFd, request: c_ulong, arg: *mut T) -> std::io::Result<c_int> {
    let ret = libc::ioctl(fd, request, arg);
    if ret < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(ret)
    }
}
