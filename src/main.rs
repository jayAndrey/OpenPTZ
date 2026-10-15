mod api;
pub mod camera;
pub mod cameras;
mod cli;
mod desktop;
mod presets;
mod tui;
mod v4l2;
mod video;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Commands, PanArgs, PresetArgs, SetArgs, TiltArgs, ZoomArgs};
use presets::PresetManager;
use tui::TuiApp;
use v4l2::CameraDevice;

fn main() -> Result<()> {
    let args = Cli::parse();

    // 1. Handle "list" command before opening a specific device
    if let Some(Commands::List) = args.command {
        list_devices();
        return Ok(());
    }

    // 2. Open camera device (custom path or auto-detect Logitech Rally / PTZ)
    let device = match &args.device {
        Some(path) => CameraDevice::open(path)?,
        None => CameraDevice::open_default()?,
    };

    // 3. Handle backward-compatibility flags with xMRi/PTZControl
    if handle_compat_flags(&device, &args)? {
        return Ok(());
    }

    // 4. Dispatch standard commands
    match args.command {
        Some(Commands::Ui { port, no_browser }) => {
            launch_web_ui(device, port, no_browser)?;
        }

        Some(Commands::Gui) => {
            desktop::run(device)?;
        }

        None | Some(Commands::Tui) => {
            let mut app = TuiApp::new(device);
            app.run()?;
        }

        Some(Commands::Status { json }) => {
            let status = device.get_status();
            if json {
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else {
                print_status_table(&status, device.profile.name(), device.has_rally_xu());
            }
        }

        Some(Commands::Home) => {
            println!("Resetting camera to Home position...");
            device.goto_home()?;
            println!("Camera is at Home position (Pan=0.0°, Tilt=0.0°, Zoom=1.0x).");
        }

        Some(Commands::Pan(pan_args)) => {
            handle_pan(&device, pan_args)?;
        }

        Some(Commands::Tilt(tilt_args)) => {
            handle_tilt(&device, tilt_args)?;
        }

        Some(Commands::Zoom(zoom_args)) => {
            handle_zoom(&device, zoom_args)?;
        }

        Some(Commands::Move {
            pan,
            tilt,
            pan_delta,
            tilt_delta,
        }) => {
            if let Some(p) = pan {
                device.set_pan_deg(p)?;
            } else if let Some(dp) = pan_delta {
                device.pan_relative(dp)?;
            }

            if let Some(t) = tilt {
                device.set_tilt_deg(t)?;
            } else if let Some(dt) = tilt_delta {
                device.tilt_relative(dt)?;
            }

            let cur_pan = device.get_pan_deg()?;
            let cur_tilt = device.get_tilt_deg()?;
            println!(
                "Current Position: Pan={:+.1}°, Tilt={:+.1}°",
                cur_pan, cur_tilt
            );
        }

        Some(Commands::Preset(preset_args)) => {
            handle_presets(&device, preset_args)?;
        }

        Some(Commands::Set(set_args)) => {
            handle_set(&device, set_args)?;
        }

        Some(Commands::Serve { port }) => {
            let server = api::ApiServer::new(device, port);
            server.run()?;
        }

        Some(Commands::TestMotion) => {
            run_motion_diagnostics(&device)?;
        }

        Some(Commands::List) => unreachable!(),
    }

    Ok(())
}

fn run_motion_diagnostics(device: &CameraDevice) -> Result<()> {
    println!("\n=======================================================");
    println!("       OpenPTZ Physical Motion & Camera Diagnostics    ");
    println!("=======================================================\n");

    let status = device.get_status();
    println!("📷 Camera:       {}", status.card_name);
    println!("📍 Device Path:  {}", status.device_path);
    println!("🎯 Profile:      {}", device.profile.name());

    let initial_pan = device.get_pan_deg()?;
    let initial_tilt = device.get_tilt_deg()?;
    let initial_zoom = device.get_zoom_x()?;

    println!("\n[1/5] Initial Position Check:");
    println!(
        "      Pan: {:+.1}°, Tilt: {:+.1}°, Zoom: {:.1}x",
        initial_pan, initial_tilt, initial_zoom
    );

    // If camera is parked in privacy mode (tilt <= -80.0°), wake it up
    if initial_tilt <= -80.0 {
        println!("\n⚠️  Camera lens is in privacy park mode (-90°). Waking up...");
        device.wake_privacy()?;
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let wake_tilt = device.get_tilt_deg()?;
        println!("      Woken up! Tilt is now: {:+.1}°", wake_tilt);
    }

    println!("\n[2/5] Testing Pan Motion:");
    println!("      -> Moving Pan Right (+15.0°)...");
    device.pan_relative(15.0)?;
    std::thread::sleep(std::time::Duration::from_millis(600));
    let pan_pos1 = device.get_pan_deg()?;
    println!("      Current Pan: {:+.1}°", pan_pos1);

    println!("      -> Moving Pan Left (-30.0°)...");
    device.pan_relative(-30.0)?;
    std::thread::sleep(std::time::Duration::from_millis(600));
    let pan_pos2 = device.get_pan_deg()?;
    println!("      Current Pan: {:+.1}°", pan_pos2);

    println!("      -> Returning Pan to Center (0.0°)...");
    device.set_pan_deg(0.0)?;
    std::thread::sleep(std::time::Duration::from_millis(500));

    println!("\n[3/5] Testing Tilt Motion:");
    println!("      -> Moving Tilt Up (+15.0°)...");
    device.tilt_relative(15.0)?;
    std::thread::sleep(std::time::Duration::from_millis(600));
    let tilt_pos1 = device.get_tilt_deg()?;
    println!("      Current Tilt: {:+.1}°", tilt_pos1);

    println!("      -> Moving Tilt Down (-15.0°)...");
    device.tilt_relative(-15.0)?;
    std::thread::sleep(std::time::Duration::from_millis(600));
    let tilt_pos2 = device.get_tilt_deg()?;
    println!("      Current Tilt: {:+.1}°", tilt_pos2);

    println!("      -> Returning Tilt to Horizon (0.0°)...");
    device.set_tilt_deg(0.0)?;
    std::thread::sleep(std::time::Duration::from_millis(500));

    println!("\n[4/5] Testing Optical Zoom:");
    println!("      -> Zooming in to 2.5x...");
    device.set_zoom_x(2.5)?;
    std::thread::sleep(std::time::Duration::from_millis(600));
    let zoom_pos1 = device.get_zoom_x()?;
    println!("      Current Zoom: {:.1}x", zoom_pos1);

    println!("      -> Zooming out to 1.0x (Wide)...");
    device.set_zoom_x(1.0)?;
    std::thread::sleep(std::time::Duration::from_millis(600));
    let zoom_pos2 = device.get_zoom_x()?;
    println!("      Current Zoom: {:.1}x", zoom_pos2);

    println!("\n[5/5] Resetting to Home Position...");
    device.goto_home()?;
    std::thread::sleep(std::time::Duration::from_millis(500));

    let final_pan = device.get_pan_deg()?;
    let final_tilt = device.get_tilt_deg()?;
    let final_zoom = device.get_zoom_x()?;
    println!(
        "      Final Position: Pan: {:+.1}°, Tilt: {:+.1}°, Zoom: {:.1}x",
        final_pan, final_tilt, final_zoom
    );

    println!("\n✅ All PTZ camera motion diagnostics completed successfully!\n");
    Ok(())
}

fn launch_web_ui(device: CameraDevice, port: u16, no_browser: bool) -> Result<()> {
    // Automatically wake up camera from privacy park mode (-90°)
    if let Ok(tilt) = device.get_tilt_deg() {
        if tilt <= -80.0 {
            println!("📷 Waking Logitech Rally from privacy parking (-90°)...");
            let _ = device.wake_privacy();
        }
    }

    if !no_browser {
        let p = port;
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let url = format!("http://localhost:{}", p);
            open_browser(&url);
        });
    }

    let server = api::ApiServer::new(device, port);
    server.run()?;
    Ok(())
}

fn open_browser(url: &str) {
    #[cfg(target_os = "windows")]
    {
        // Try Microsoft Edge or Chrome in app mode first
        let edge_res = std::process::Command::new("msedge")
            .arg(format!("--app={}", url))
            .arg("--autoplay-policy=no-user-gesture-required")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();

        if edge_res.is_err() {
            let chrome_res = std::process::Command::new("chrome")
                .arg(format!("--app={}", url))
                .arg("--autoplay-policy=no-user-gesture-required")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();

            if chrome_res.is_err() {
                let _ = std::process::Command::new("cmd")
                    .args(["/C", "start", "", url])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let chrome_res = std::process::Command::new(
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        )
        .arg(format!("--app={}", url))
        .arg("--autoplay-policy=no-user-gesture-required")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();

        if chrome_res.is_err() {
            let _ = std::process::Command::new("open")
                .arg(url)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        }
    }

    #[cfg(target_os = "linux")]
    {
        let chrome_res = std::process::Command::new("google-chrome")
            .arg(format!("--app={}", url))
            .arg("--autoplay-policy=no-user-gesture-required")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();

        if chrome_res.is_err() {
            let _ = std::process::Command::new("xdg-open")
                .arg(url)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

fn list_devices() {
    let devices = CameraDevice::discover_devices();
    println!("Found {} video capture device(s):", devices.len());
    println!("{:-<75}", "");
    println!(
        "{:<14} {:<30} {:<12} {:<10}",
        "Device", "Name", "PTZ", "Rally XU"
    );
    println!("{:-<75}", "");

    for d in devices {
        let ptz_str = if d.supports_ptz { "YES" } else { "no" };
        let rally_str = if d.is_rally { "LOGI RALLY" } else { "-" };
        println!(
            "{:<14} {:<30} {:<12} {:<10}",
            d.path.display(),
            d.card_name,
            ptz_str,
            rally_str
        );
    }
    println!("{:-<75}", "");
}

fn print_status_table(status: &v4l2::CameraStatus, profile_name: &str, has_xu: bool) {
    println!("\n🎥 Camera Status: {}", status.card_name);
    println!("   Profile:       {}", profile_name);
    println!("   Device Path:   {}", status.device_path);
    println!(
        "   Logitech XU:   {}",
        if has_xu {
            "Active (HW Presets 1-8 supported)"
        } else {
            "Standard V4L2"
        }
    );
    println!("{:-<50}", "");
    println!(
        "   Pan:           {:+.1}°  (raw: {})",
        status.pan_deg, status.pan_raw
    );
    println!(
        "   Tilt:          {:+.1}°  (raw: {})",
        status.tilt_deg, status.tilt_raw
    );
    println!(
        "   Zoom:          {:.1}x    (raw: {})",
        status.zoom_x, status.zoom_raw
    );
    println!("{:-<50}", "");
    if let Some(f) = status.power_line_frequency {
        let text = match f {
            1 => "50 Hz (PAL / Europe)",
            2 => "60 Hz (NTSC / Americas)",
            _ => "Disabled (0)",
        };
        println!("   Anti-Flicker:  {} ({})", text, f);
    }
    if let Some(b) = status.brightness {
        println!("   Brightness:    {}", b);
    }
    if let Some(c) = status.contrast {
        println!("   Contrast:      {}", c);
    }
    if let Some(s) = status.saturation {
        println!("   Saturation:    {}", s);
    }
    if let Some(exp) = status.auto_exposure {
        let text = if exp == 3 {
            "Auto (Aperture Priority)"
        } else {
            "Manual"
        };
        println!("   Exposure Mode: {}", text);
    }
    if let Some(exp_time) = status.exposure_time {
        println!("   Exposure Time: {}", exp_time);
    }
    if let Some(af) = status.auto_focus {
        println!(
            "   Auto Focus:    {}",
            if af { "Enabled" } else { "Disabled" }
        );
    }
    println!("{:-<50}\n", "");
}

fn handle_pan(device: &CameraDevice, args: PanArgs) -> Result<()> {
    if let Some(to) = args.to {
        device.set_pan_deg(to)?;
        println!("Pan set to {:+.1}°", to);
    } else if args.left {
        let pos = device.pan_relative(-args.step)?;
        println!("Pan Left by {:.1}° -> Current: {:+.1}°", args.step, pos);
    } else if args.right {
        let pos = device.pan_relative(args.step)?;
        println!("Pan Right by {:.1}° -> Current: {:+.1}°", args.step, pos);
    } else {
        let pos = device.get_pan_deg()?;
        println!("Current Pan: {:+.1}°", pos);
    }
    Ok(())
}

fn handle_tilt(device: &CameraDevice, args: TiltArgs) -> Result<()> {
    if let Some(to) = args.to {
        device.set_tilt_deg(to)?;
        println!("Tilt set to {:+.1}°", to);
    } else if args.up {
        let pos = device.tilt_relative(args.step)?;
        println!("Tilt Up by {:.1}° -> Current: {:+.1}°", args.step, pos);
    } else if args.down {
        let pos = device.tilt_relative(-args.step)?;
        println!("Tilt Down by {:.1}° -> Current: {:+.1}°", args.step, pos);
    } else {
        let pos = device.get_tilt_deg()?;
        println!("Current Tilt: {:+.1}°", pos);
    }
    Ok(())
}

fn handle_zoom(device: &CameraDevice, args: ZoomArgs) -> Result<()> {
    if let Some(to) = args.to {
        device.set_zoom_x(to)?;
        println!("Zoom set to {:.1}x", to);
    } else if args.zoom_in {
        let z = device.zoom_relative(args.step)?;
        println!("Zoom In by {:.1}x -> Current: {:.1}x", args.step, z);
    } else if args.zoom_out {
        let z = device.zoom_relative(-args.step)?;
        println!("Zoom Out by {:.1}x -> Current: {:.1}x", args.step, z);
    } else {
        let z = device.get_zoom_x()?;
        println!("Current Zoom: {:.1}x", z);
    }
    Ok(())
}

fn handle_presets(device: &CameraDevice, args: PresetArgs) -> Result<()> {
    let mut pm = PresetManager::load();

    if let Some(p) = args.recall {
        println!("Recalling Preset {}...", p);
        if device.profile.capabilities().has_hardware_presets && device.has_rally_xu() {
            if let Ok(()) = device.recall_hardware_preset(p) {
                println!("Camera moved to Hardware Preset {} (via camera EEPROM).", p);
                return Ok(());
            }
        }
        match pm.recall_slot(p, device) {
            Ok(preset) => {
                println!(
                    "Camera moved to Preset {}: Pan={:+.1}°, Tilt={:+.1}°, Zoom={:.1}x",
                    p, preset.pan_deg, preset.tilt_deg, preset.zoom_x
                );
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        }
        return Ok(());
    }

    if let Some(p) = args.save {
        println!("Saving current position to Preset {}...", p);
        if device.profile.capabilities().has_hardware_presets && device.has_rally_xu() {
            if let Ok(()) = device.save_hardware_preset(p) {
                println!(
                    "Saved position to Hardware Preset {} (via camera EEPROM).",
                    p
                );
            }
        }
        let preset = pm.save_slot(p, device)?;
        println!(
            "Saved position to Preset {}: Pan={:+.1}°, Tilt={:+.1}°, Zoom={:.1}x",
            p, preset.pan_deg, preset.tilt_deg, preset.zoom_x
        );
        return Ok(());
    }

    if let Some(name) = args.save_soft {
        let pan = device.get_pan_deg()?;
        let tilt = device.get_tilt_deg()?;
        let zoom = device.get_zoom_x()?;
        let brightness = device.get_brightness().ok();
        let freq = device.get_power_line_frequency().ok();

        pm.set_preset(&name, pan, tilt, zoom, brightness, freq)?;
        println!(
            "Saved preset '{}': Pan={:+.1}°, Tilt={:+.1}°, Zoom={:.1}x",
            name, pan, tilt, zoom
        );
        return Ok(());
    }

    if let Some(name) = args.recall_soft {
        match pm.apply(device, &name) {
            Ok(preset) => {
                println!(
                    "Applied preset '{}': Pan={:+.1}°, Tilt={:+.1}°, Zoom={:.1}x",
                    name, preset.pan_deg, preset.tilt_deg, preset.zoom_x
                );
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        }
        return Ok(());
    }

    if let Some(name) = args.delete_soft {
        if pm.delete_preset(&name)? {
            println!("Deleted preset '{}'.", name);
        } else {
            println!("Preset '{}' did not exist.", name);
        }
        return Ok(());
    }

    if args.list_soft {
        let list = pm.list();
        println!("Presets ({}):", list.len());
        for p in list {
            println!(
                "  • {:<15} Pan: {:+.1}°, Tilt: {:+.1}°, Zoom: {:.1}x",
                p.name, p.pan_deg, p.tilt_deg, p.zoom_x
            );
        }
        return Ok(());
    }

    let list = pm.list();
    println!("Presets ({}) stored in config:", list.len());
    for p in list {
        println!(
            "  • Slot {:<8} Pan: {:+.1}°, Tilt: {:+.1}°, Zoom: {:.1}x",
            p.name, p.pan_deg, p.tilt_deg, p.zoom_x
        );
    }
    println!("\nUsage:");
    println!("  openptz preset --save <1-8>");
    println!("  openptz preset --recall <1-8>");
    println!("  openptz preset --save-soft <name>");
    println!("  openptz preset --recall-soft <name>");
    Ok(())
}

fn handle_set(device: &CameraDevice, args: SetArgs) -> Result<()> {
    if let Some(hz) = args.flicker {
        device.set_power_line_frequency(hz)?;
        let desc = match hz {
            0 => "Disabled",
            50 => "50 Hz (PAL / Europe)",
            60 => "60 Hz (NTSC / Americas)",
            _ => "Custom",
        };
        println!("Anti-Flicker (Power Line Frequency) set to: {}", desc);
    }

    if let Some(b) = args.brightness {
        device.set_brightness(b)?;
        println!("Brightness set to: {}", b);
    }

    if let Some(c) = args.contrast {
        device.set_contrast(c)?;
        println!("Contrast set to: {}", c);
    }

    if let Some(s) = args.saturation {
        device.set_saturation(s)?;
        println!("Saturation set to: {}", s);
    }

    if let Some(sh) = args.sharpness {
        device.set_sharpness(sh)?;
        println!("Sharpness set to: {}", sh);
    }

    if let Some(ae) = args.auto_exposure {
        device.set_auto_exposure(ae)?;
        println!("Auto Exposure set to: {}", ae);
    }

    if let Some(exp) = args.exposure_time {
        device.set_exposure_time(exp)?;
        println!("Exposure Time set to: {}", exp);
    }

    if let Some(af) = args.auto_focus {
        device.set_auto_focus(af)?;
        println!("Auto Focus set to: {}", af);
    }

    Ok(())
}

fn handle_compat_flags(device: &CameraDevice, args: &Cli) -> Result<bool> {
    if args.compat_move_home {
        device.goto_home()?;
        return Ok(true);
    }
    if args.compat_move_left {
        device.pan_relative(-5.0)?;
        return Ok(true);
    }
    if args.compat_move_right {
        device.pan_relative(5.0)?;
        return Ok(true);
    }
    if args.compat_move_up {
        device.tilt_relative(5.0)?;
        return Ok(true);
    }
    if args.compat_move_down {
        device.tilt_relative(-5.0)?;
        return Ok(true);
    }
    if args.compat_zoom_in {
        device.zoom_relative(0.5)?;
        return Ok(true);
    }
    if args.compat_zoom_out {
        device.zoom_relative(-0.5)?;
        return Ok(true);
    }
    if let Some(p) = args.compat_store {
        let mut pm = PresetManager::load();
        pm.save_slot(p, device)?;
        return Ok(true);
    }
    if let Some(p) = args.compat_restore {
        let pm = PresetManager::load();
        pm.recall_slot(p, device)?;
        return Ok(true);
    }

    Ok(false)
}
