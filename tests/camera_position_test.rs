use openptz::cameras::traits::CameraProfile;
use openptz::cameras::{detect_profile, LogitechRallyProfile};
use openptz::presets::PresetManager;

#[test]
fn test_logitech_rally_pan_tilt_zoom_limits() {
    let profile = LogitechRallyProfile::new();
    let limits = profile.limits();

    // 1. Verify Pan mechanical limits (-90° to +90°)
    let min_pan_deg = limits.pan_raw_to_deg(limits.pan_min);
    let max_pan_deg = limits.pan_raw_to_deg(limits.pan_max);
    assert!(
        (min_pan_deg - (-90.0)).abs() < 0.01,
        "Min pan must be -90.0°"
    );
    assert!((max_pan_deg - 90.0).abs() < 0.01, "Max pan must be +90.0°");

    // 2. Verify Tilt mechanical limits (-90° to +50°)
    let min_tilt_deg = limits.tilt_raw_to_deg(limits.tilt_min);
    let max_tilt_deg = limits.tilt_raw_to_deg(limits.tilt_max);
    assert!(
        (min_tilt_deg - (-90.0)).abs() < 0.01,
        "Min tilt must be -90.0°"
    );
    assert!(
        (max_tilt_deg - 50.0).abs() < 0.01,
        "Max tilt must be +50.0°"
    );

    // 3. Verify Zoom limits (1.0x to 15.0x)
    let min_zoom = limits.zoom_raw_to_x(limits.zoom_min);
    let max_zoom = limits.zoom_raw_to_x(limits.zoom_max);
    assert!((min_zoom - 1.0).abs() < 0.01, "Min zoom must be 1.0x");
    assert!((max_zoom - 15.0).abs() < 0.01, "Max zoom must be 15.0x");
}

#[test]
fn test_coordinate_conversion_roundtrips() {
    let profile = LogitechRallyProfile::new();
    let limits = profile.limits();

    // Pan degree to raw and back
    let test_angles = [-90.0, -45.0, -10.5, 0.0, 15.2, 45.0, 90.0];
    for &deg in &test_angles {
        let raw = limits.deg_to_pan_raw(deg);
        let back = limits.pan_raw_to_deg(raw);
        assert!(
            (back - deg).abs() < 0.01,
            "Pan roundtrip failed for {deg}°: got {back}°"
        );
    }

    // Tilt degree to raw and back
    let test_tilts = [-90.0, -30.0, 0.0, 25.0, 50.0];
    for &deg in &test_tilts {
        let raw = limits.deg_to_tilt_raw(deg);
        let back = limits.tilt_raw_to_deg(raw);
        assert!(
            (back - deg).abs() < 0.01,
            "Tilt roundtrip failed for {deg}°: got {back}°"
        );
    }

    // Zoom factor to raw and back
    let test_zooms = [1.0, 1.5, 2.0, 5.0, 10.0, 15.0];
    for &zoom in &test_zooms {
        let raw = limits.zoom_x_to_raw(zoom);
        let back = limits.zoom_raw_to_x(raw);
        assert!(
            (back - zoom).abs() < 0.01,
            "Zoom roundtrip failed for {zoom}x: got {back}x"
        );
    }
}

#[test]
fn test_boundary_clamping() {
    let profile = LogitechRallyProfile::new();
    let limits = profile.limits();

    // Pan clamping beyond mechanical boundaries
    assert_eq!(limits.deg_to_pan_raw(-180.0), limits.pan_min);
    assert_eq!(limits.deg_to_pan_raw(180.0), limits.pan_max);

    // Tilt clamping beyond mechanical boundaries
    assert_eq!(limits.deg_to_tilt_raw(-120.0), limits.tilt_min);
    assert_eq!(limits.deg_to_tilt_raw(90.0), limits.tilt_max);

    // Zoom clamping beyond optical limits
    assert_eq!(limits.zoom_x_to_raw(0.5), limits.zoom_min);
    assert_eq!(limits.zoom_x_to_raw(30.0), limits.zoom_max);
}

#[test]
fn test_camera_profile_detection() {
    let rally_profile = detect_profile("Logi Rally Camera", Some((0x046d, 0x0881)));
    assert_eq!(rally_profile.name(), "Logitech Rally");

    let generic_profile = detect_profile("Generic USB Webcam", Some((0x1234, 0x5678)));
    assert_eq!(generic_profile.name(), "Generic UVC");
}

#[test]
fn test_preset_coordinate_tracking() {
    let temp_dir = std::env::temp_dir();
    let temp_preset_file =
        temp_dir.join(format!("openptz_test_presets_{}.json", std::process::id()));
    let mut pm = PresetManager::with_path(temp_preset_file.clone());

    // Save and verify preset positions
    pm.set_preset("Speaker", 25.0, 10.0, 3.5, Some(128), Some(1))
        .expect("Must save Speaker preset");

    pm.set_preset("Whiteboard", -45.0, -5.0, 5.0, None, None)
        .expect("Must save Whiteboard preset");

    let list = pm.list();
    assert_eq!(list.len(), 2);

    let recalled_p1 = pm.get_preset("Speaker").expect("Preset Speaker must exist");
    assert!((recalled_p1.pan_deg - 25.0).abs() < 0.001);
    assert!((recalled_p1.tilt_deg - 10.0).abs() < 0.001);
    assert!((recalled_p1.zoom_x - 3.5).abs() < 0.001);

    // Test overwrite preset
    pm.set_preset("Speaker", 30.0, 12.0, 4.0, Some(140), Some(1))
        .expect("Must overwrite Speaker preset");
    let recalled_updated = pm.get_preset("Speaker").unwrap();
    assert!((recalled_updated.pan_deg - 30.0).abs() < 0.001);

    // Test deletion
    pm.delete_preset("Whiteboard")
        .expect("Must delete Whiteboard preset");
    assert_eq!(pm.list().len(), 1);
    assert!(pm.get_preset("Whiteboard").is_none());

    let _ = std::fs::remove_file(temp_preset_file);
}

#[test]
fn test_motion_delta_calculation() {
    // Test moving between positions
    let current_pan = 0.0f32;
    let target_pan = 15.0f32;
    let delta = target_pan - current_pan;
    assert!(delta > 0.0, "Delta towards right must be positive");
    assert!((delta - 15.0).abs() < 0.001);

    let current_pan = 15.0f32;
    let target_pan = -10.0f32;
    let delta = target_pan - current_pan;
    assert!(delta < 0.0, "Delta towards left must be negative");
    assert!((delta - (-25.0)).abs() < 0.001);

    // Step accumulation
    let mut position = 0.0f32;
    let step = 5.0f32;
    for _ in 0..4 {
        position += step;
    }
    assert!((position - 20.0).abs() < 0.001);
}
