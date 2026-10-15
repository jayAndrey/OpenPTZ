#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "linux")]
pub use linux::CameraDeviceImpl as CameraDevice;

#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "windows")]
pub use windows::CameraDeviceImpl as CameraDevice;

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
pub use macos::CameraDeviceImpl as CameraDevice;

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub mod fallback;
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub use fallback::CameraDeviceImpl as CameraDevice;
