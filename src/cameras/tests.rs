use std::sync::Arc;

use crate::cameras::detect_profile;
use crate::cameras::generic::GenericUvcProfile;
use crate::cameras::logitech_rally::{
    is_selector_2_safe, LogitechRallyProfile, LOGITECH_VENDOR_ID, RALLY_CAMERA_PRODUCT_ID,
    RALLY_PAN_MAX, RALLY_PAN_MIN, RALLY_TILT_MAX, RALLY_TILT_MIN, RALLY_ZOOM_MAX, RALLY_ZOOM_MIN,
};
use crate::cameras::registry::CameraRegistry;
use crate::cameras::traits::{CameraCapabilities, CameraProfile, PtzLimits};

#[test]
fn test_ptz_limits_pan_mapping() {
    let limits = PtzLimits {
        pan_min: -324000,
        pan_max: 324000,
        pan_step: 3600,
        tilt_min: -324000,
        tilt_max: 180000,
        tilt_step: 3600,
        zoom_min: 100,
        zoom_max: 1500,
        zoom_step: 1,
    };

    assert_eq!(limits.pan_raw_to_deg(0), 0.0);
    assert_eq!(limits.pan_raw_to_deg(3600), 1.0);
    assert_eq!(limits.pan_raw_to_deg(-324000), -90.0);
    assert_eq!(limits.pan_raw_to_deg(324000), 90.0);

    assert_eq!(limits.deg_to_pan_raw(0.0), 0);
    assert_eq!(limits.deg_to_pan_raw(1.0), 3600);
    assert_eq!(limits.deg_to_pan_raw(45.0), 162000);
    assert_eq!(limits.deg_to_pan_raw(-90.0), -324000);
    assert_eq!(limits.deg_to_pan_raw(90.0), 324000);

    // Clamping bounds
    assert_eq!(limits.deg_to_pan_raw(120.0), 324000);
    assert_eq!(limits.deg_to_pan_raw(-120.0), -324000);
}

#[test]
fn test_ptz_limits_tilt_mapping() {
    let limits = PtzLimits {
        pan_min: -324000,
        pan_max: 324000,
        pan_step: 3600,
        tilt_min: -324000, // -90.0°
        tilt_max: 180000,  // +50.0°
        tilt_step: 3600,
        zoom_min: 100,
        zoom_max: 1500,
        zoom_step: 1,
    };

    assert_eq!(limits.tilt_raw_to_deg(0), 0.0);
    assert_eq!(limits.tilt_raw_to_deg(-324000), -90.0);
    assert_eq!(limits.tilt_raw_to_deg(180000), 50.0);

    assert_eq!(limits.deg_to_tilt_raw(0.0), 0);
    assert_eq!(limits.deg_to_tilt_raw(-90.0), -324000);
    assert_eq!(limits.deg_to_tilt_raw(50.0), 180000);

    // Clamping bounds: asymmetric tilt (-90° to +50°)
    assert_eq!(limits.deg_to_tilt_raw(75.0), 180000);
    assert_eq!(limits.deg_to_tilt_raw(-110.0), -324000);
}

#[test]
fn test_ptz_limits_zoom_mapping() {
    let limits = PtzLimits {
        pan_min: -324000,
        pan_max: 324000,
        pan_step: 3600,
        tilt_min: -324000,
        tilt_max: 180000,
        tilt_step: 3600,
        zoom_min: 100,
        zoom_max: 1500,
        zoom_step: 1,
    };

    assert_eq!(limits.zoom_raw_to_x(100), 1.0);
    assert_eq!(limits.zoom_raw_to_x(1500), 15.0);
    assert_eq!(limits.zoom_raw_to_x(350), 3.5);

    assert_eq!(limits.zoom_x_to_raw(1.0), 100);
    assert_eq!(limits.zoom_x_to_raw(15.0), 1500);
    assert_eq!(limits.zoom_x_to_raw(3.5), 350);

    // Clamping bounds
    assert_eq!(limits.zoom_x_to_raw(0.5), 100);
    assert_eq!(limits.zoom_x_to_raw(20.0), 1500);
}

#[test]
fn test_logitech_rally_profile() {
    let profile = LogitechRallyProfile::new();
    assert_eq!(profile.name(), "Logitech Rally");

    let caps = profile.capabilities();
    assert!(caps.has_pan_tilt);
    assert!(caps.has_zoom);
    assert!(caps.has_hardware_presets);
    assert_eq!(caps.hardware_preset_count, 8);
    assert!(caps.has_privacy_mode);
    assert_eq!(caps.vendor_name, "Logitech");
    assert_eq!(caps.model_name, "Rally Camera");

    let limits = profile.limits();
    assert_eq!(limits.pan_min, RALLY_PAN_MIN);
    assert_eq!(limits.pan_max, RALLY_PAN_MAX);
    assert_eq!(limits.tilt_min, RALLY_TILT_MIN);
    assert_eq!(limits.tilt_max, RALLY_TILT_MAX);
    assert_eq!(limits.zoom_min, RALLY_ZOOM_MIN);
    assert_eq!(limits.zoom_max, RALLY_ZOOM_MAX);

    // Matching
    assert!(profile.matches("Logitech Rally Camera", None));
    assert!(profile.matches("rally plus", None));
    assert!(profile.matches(
        "Unknown Name",
        Some((LOGITECH_VENDOR_ID, RALLY_CAMERA_PRODUCT_ID))
    ));
    assert!(!profile.matches("Logitech HD Pro Webcam C920", None));
    assert!(!profile.matches("Generic Camera", Some((0x1234, 0x5678))));
}

#[test]
fn test_selector_2_quirk_bounds() {
    // Safe values: 1 (pan reset), 2 (tilt reset), 3 (home), 4..11 (save presets 1..8), 12..19 (recall presets 1..8)
    assert!(!is_selector_2_safe(0));
    assert!(is_selector_2_safe(1));
    assert!(is_selector_2_safe(3)); // Home
    assert!(is_selector_2_safe(4)); // Save Preset 1
    assert!(is_selector_2_safe(11)); // Save Preset 8
    assert!(is_selector_2_safe(12)); // Recall Preset 1
    assert!(is_selector_2_safe(19)); // Recall Preset 8
    assert!(!is_selector_2_safe(20));
    assert!(!is_selector_2_safe(255));
}

#[test]
fn test_generic_uvc_profile() {
    let profile = GenericUvcProfile::new();
    assert_eq!(profile.name(), "Generic UVC");

    let caps = profile.capabilities();
    assert!(caps.has_pan_tilt);
    assert!(!caps.has_hardware_presets);
    assert_eq!(caps.hardware_preset_count, 0);
    assert_eq!(caps.vendor_name, "Generic");

    // Fallback matches everything
    assert!(profile.matches("Any Camera", None));

    // Hardware presets unsupported
    assert!(profile.recall_hardware_preset(0, 1).is_err());
    assert!(profile.save_hardware_preset(0, 1).is_err());
}

#[test]
fn test_registry_detection() {
    let rally = detect_profile("Logitech Rally Camera", None);
    assert_eq!(rally.name(), "Logitech Rally");

    let generic = detect_profile("OBS Virtual Camera", None);
    assert_eq!(generic.name(), "Generic UVC");

    let mut reg = CameraRegistry::new();
    assert_eq!(reg.profiles().len(), 2);

    // Custom profile registration
    struct CustomProfile;
    impl CameraProfile for CustomProfile {
        fn name(&self) -> &str {
            "Custom Test Profile"
        }
        fn capabilities(&self) -> CameraCapabilities {
            CameraCapabilities::default()
        }
        fn limits(&self) -> PtzLimits {
            GenericUvcProfile::new().limits()
        }
        fn matches(&self, card_name: &str, _usb_vid_pid: Option<(u16, u16)>) -> bool {
            card_name.contains("CustomCam")
        }
    }

    reg.register(Arc::new(CustomProfile));
    assert_eq!(reg.profiles().len(), 3);
    let found = reg.find("CustomCam Model X", None);
    assert_eq!(found.name(), "Custom Test Profile");
}
