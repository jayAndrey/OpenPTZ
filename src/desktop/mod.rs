use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Result};
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions, Vec2};

use crate::presets::PresetManager;
use crate::v4l2::{CameraDevice, CameraStatus};

const RESOLUTIONS: [(u32, u32); 3] = [(640, 480), (1280, 720), (1920, 1080)];
/// Half field of view at 1x zoom, matching the web UI click-to-aim.
const HALF_FOV_PAN_DEG: f32 = 20.0;
const HALF_FOV_TILT_DEG: f32 = 14.0;
const MIN_AIM_DEG: f32 = 0.5;

/// Convert a click position (0..=1 from the top-left of the image) into pan/tilt
/// deltas in degrees. Positive tilt is up.
pub fn click_to_delta(norm_x: f32, norm_y: f32, zoom: f32) -> (f32, f32) {
    let nx = ((norm_x - 0.5) * 2.0).clamp(-1.0, 1.0);
    let ny = ((norm_y - 0.5) * 2.0).clamp(-1.0, 1.0);
    let zoom = if zoom.is_finite() { zoom } else { 1.0 };
    let fov = 1.0 / (zoom * 0.7).max(1.0);
    (nx * HALF_FOV_PAN_DEG * fov, -ny * HALF_FOV_TILT_DEG * fov)
}

/// Format unix seconds as YYYYMMDD-HHMMSS (UTC).
fn timestamp_name(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn save_snapshot(frame: &VideoFrame) -> Result<std::path::PathBuf> {
    let dir = dirs::picture_dir()
        .or_else(dirs::home_dir)
        .ok_or_else(|| anyhow!("no Pictures or home directory found"))?;
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = dir.join(format!("openptz-{}.jpg", timestamp_name(secs)));
    std::fs::write(&path, &frame.jpeg[..])
        .map_err(|e| anyhow!("could not write {}: {e}", path.display()))?;
    Ok(path)
}
use crate::video::{VideoFrame, VideoHub};

type PtzAction = Box<dyn FnOnce(&CameraDevice) -> Result<()> + Send>;

enum Command {
    Run(PtzAction),
    Aim(f32, f32),
    Recall(u8),
    Save(u8),
    SelectCamera(String),
    SetResolution((u32, u32)),
}

/// State published by the PTZ worker thread and read by the UI thread.
struct WorkerOutput {
    status: CameraStatus,
    message: Option<String>,
    video: Arc<VideoHub>,
    resolution: (u32, u32),
}

type Shared = Arc<Mutex<WorkerOutput>>;

fn lock(shared: &Shared) -> std::sync::MutexGuard<'_, WorkerOutput> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn run(device: CameraDevice) -> Result<()> {
    let status = device.get_status();
    let resolution = (1280, 720);
    let video = VideoHub::start_with(
        status.device_path.clone(),
        status.card_name.clone(),
        true,
        Some(resolution),
    );
    let shared: Shared = Arc::new(Mutex::new(WorkerOutput {
        status,
        message: None,
        video,
        resolution,
    }));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 760.0])
            .with_drag_and_drop(false),
        ..Default::default()
    };
    eframe::run_native(
        "OpenPTZ",
        options,
        Box::new(move |context| {
            let (sender, receiver) = mpsc::channel();
            let worker_shared = Arc::clone(&shared);
            let egui_context = context.egui_ctx.clone();
            std::thread::spawn(move || {
                worker_loop(device, receiver, worker_shared, egui_context);
            });
            Ok(Box::new(DesktopApp::new(sender, shared)))
        }),
    )
    .map_err(|error| anyhow!("Could not launch native GUI: {error}"))
}

/// Owns the camera device: runs PTZ commands, camera switches and status polling
/// so the UI thread never blocks on hardware I/O.
fn worker_loop(
    mut device: CameraDevice,
    receiver: mpsc::Receiver<Command>,
    shared: Shared,
    context: egui::Context,
) {
    let mut presets = PresetManager::load();
    loop {
        let command = match receiver.recv_timeout(Duration::from_millis(800)) {
            Ok(command) => Some(command),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let message =
            command.map(|command| handle_command(command, &mut device, &mut presets, &shared));
        let status = device.get_status();
        {
            let mut output = lock(&shared);
            output.status = status;
            if let Some(message) = message {
                output.message = message;
            }
        }
        context.request_repaint();
    }
}

/// Executes one command and returns the error message to display (if any).
fn handle_command(
    command: Command,
    device: &mut CameraDevice,
    presets: &mut PresetManager,
    shared: &Shared,
) -> Option<String> {
    match command {
        Command::Run(action) => action(device).err().map(|error| error.to_string()),
        Command::Aim(norm_x, norm_y) => {
            let zoom = lock(shared).status.zoom_x;
            let (dp, dt) = click_to_delta(norm_x, norm_y, zoom);
            let mut errors = Vec::new();
            if dp.abs() >= MIN_AIM_DEG {
                if let Err(e) = device.pan_relative(dp) {
                    errors.push(format!("pan: {e:#}"));
                }
            }
            if dt.abs() >= MIN_AIM_DEG {
                if let Err(e) = device.tilt_relative(dt) {
                    errors.push(format!("tilt: {e:#}"));
                }
            }
            (!errors.is_empty()).then(|| format!("PTZ error: {}", errors.join("; ")))
        }
        Command::Recall(slot) => presets
            .recall_slot(slot, device)
            .err()
            .map(|error| error.to_string()),
        Command::Save(slot) => presets
            .save_slot(slot, device)
            .err()
            .map(|error| error.to_string()),
        Command::SetResolution(resolution) => {
            let status = device.get_status();
            restart_video(shared, &status, resolution);
            None
        }
        Command::SelectCamera(path) => match CameraDevice::open(&path) {
            Ok(new_device) => {
                *device = new_device;
                let status = device.get_status();
                let resolution = lock(shared).resolution;
                restart_video(shared, &status, resolution);
                None
            }
            Err(error) => Some(format!("Could not open camera: {error:#}")),
        },
    }
}

/// Stops the old capture thread (releasing the camera) before starting a new hub.
fn restart_video(shared: &Shared, status: &CameraStatus, resolution: (u32, u32)) {
    let old = Arc::clone(&lock(shared).video);
    old.stop_and_join();
    let video = VideoHub::start_with(
        status.device_path.clone(),
        status.card_name.clone(),
        true,
        Some(resolution),
    );
    let mut output = lock(shared);
    output.video = video;
    output.resolution = resolution;
    output.status = status.clone();
}

struct DesktopApp {
    sender: mpsc::Sender<Command>,
    shared: Shared,
    video: Arc<VideoHub>,
    status: CameraStatus,
    worker_message: Option<String>,
    last_frame: u64,
    video_fps: f32,
    texture: Option<TextureHandle>,
    step: f32,
    message: Option<String>,
    info: Option<String>,
    frame: Option<Arc<VideoFrame>>,
    resolution: (u32, u32),
    sources: Vec<(String, String)>,
}

impl DesktopApp {
    fn new(sender: mpsc::Sender<Command>, shared: Shared) -> Self {
        let (video, status, resolution) = {
            let output = lock(&shared);
            (
                Arc::clone(&output.video),
                output.status.clone(),
                output.resolution,
            )
        };
        Self {
            sender,
            shared,
            video,
            status,
            worker_message: None,
            last_frame: 0,
            video_fps: 0.0,
            texture: None,
            step: 5.0,
            message: None,
            info: None,
            frame: None,
            resolution,
            sources: CameraDevice::discover_devices()
                .into_iter()
                .filter(|d| d.supports_ptz || d.is_rally)
                .map(|d| (d.path.to_string_lossy().into_owned(), d.card_name))
                .collect(),
        }
    }

    fn send(&self, command: Command) {
        let _ = self.sender.send(command);
    }

    /// Copies the worker's published state; resets video state when the hub changed.
    fn sync_worker(&mut self) {
        let (status, message, video, resolution) = {
            let output = lock(&self.shared);
            (
                output.status.clone(),
                output.message.clone(),
                Arc::clone(&output.video),
                output.resolution,
            )
        };
        self.status = status;
        self.worker_message = message;
        self.resolution = resolution;
        if !Arc::ptr_eq(&self.video, &video) {
            self.video = video;
            self.texture = None;
            self.frame = None;
            self.last_frame = 0;
        }
    }

    fn snapshot(&mut self) {
        match &self.frame {
            Some(frame) => match save_snapshot(frame) {
                Ok(path) => {
                    self.info = Some(format!("Saved {}", path.display()));
                    self.message = None;
                }
                Err(error) => self.message = Some(format!("Snapshot failed: {error:#}")),
            },
            None => self.message = Some("Snapshot failed: no video frame yet".into()),
        }
    }

    fn command(&self, action: impl FnOnce(&CameraDevice) -> Result<()> + Send + 'static) {
        self.send(Command::Run(Box::new(action)));
    }

    fn refresh_video(&mut self, context: &egui::Context) -> Option<String> {
        let (sequence, frame, error) = self.video.latest();
        self.video_fps = self.video.fps();
        if sequence != self.last_frame {
            if let Some(frame) = frame {
                self.frame = Some(Arc::clone(&frame));
                let Some(rgb) = &frame.rgb else {
                    return None;
                };
                let image = ColorImage::from_rgb([frame.width, frame.height], rgb);
                if let Some(texture) = self.texture.as_mut() {
                    texture.set(image, TextureOptions::LINEAR);
                } else {
                    self.texture =
                        Some(context.load_texture("camera", image, TextureOptions::LINEAR));
                }
                self.last_frame = sequence;
            }
        }
        if error.is_some() {
            self.texture = None;
            self.frame = None;
        }
        error
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("PTZ");
        ui.label("Position (reported by device):");
        ui.label(format!("Pan: {:+.1}°", self.status.pan_deg));
        ui.label(format!("Tilt: {:+.1}°", self.status.tilt_deg));
        ui.label(format!("Zoom: {:.1}×", self.status.zoom_x));
        ui.add(egui::Slider::new(&mut self.step, 0.5..=15.0).text("Step °"));
        ui.add_space(12.0);

        ui.horizontal(|ui| {
            ui.add_space(61.0);
            if ui.button("▲").clicked() {
                let step = self.step;
                self.command(move |camera| camera.tilt_relative(step).map(|_| ()));
            }
        });
        ui.horizontal(|ui| {
            if ui.button("◀").clicked() {
                let step = self.step;
                self.command(move |camera| camera.pan_relative(-step).map(|_| ()));
            }
            if ui.button("Home").clicked() {
                self.command(CameraDevice::goto_home);
            }
            if ui.button("▶").clicked() {
                let step = self.step;
                self.command(move |camera| camera.pan_relative(step).map(|_| ()));
            }
        });
        ui.horizontal(|ui| {
            ui.add_space(61.0);
            if ui.button("▼").clicked() {
                let step = self.step;
                self.command(move |camera| camera.tilt_relative(-step).map(|_| ()));
            }
        });

        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Zoom −").clicked() {
                self.command(move |camera| camera.zoom_relative(-0.5).map(|_| ()));
            }
            if ui.button("Zoom +").clicked() {
                self.command(move |camera| camera.zoom_relative(0.5).map(|_| ()));
            }
        });
        if ui.button("Wake camera").clicked() {
            self.command(CameraDevice::wake_privacy);
        }

        ui.separator();
        ui.heading("Video");
        let mut restart = None;
        let mut camera = None;
        if !self.sources.is_empty() {
            let current = self.status.device_path.clone();
            egui::ComboBox::from_label("Camera")
                .selected_text(self.status.card_name.clone())
                .show_ui(ui, |ui| {
                    for (path, name) in &self.sources {
                        if ui.selectable_label(*path == current, name).clicked() && *path != current
                        {
                            camera = Some(path.clone());
                        }
                    }
                });
        }
        egui::ComboBox::from_label("Resolution")
            .selected_text(format!("{}x{}", self.resolution.0, self.resolution.1))
            .show_ui(ui, |ui| {
                for r in RESOLUTIONS {
                    if ui
                        .selectable_label(self.resolution == r, format!("{}x{}", r.0, r.1))
                        .clicked()
                        && self.resolution != r
                    {
                        restart = Some(r);
                    }
                }
            });
        if let Some(r) = restart {
            self.send(Command::SetResolution(r));
        }
        if let Some(path) = camera {
            self.send(Command::SelectCamera(path));
        }
        if ui.button("Snapshot").clicked() {
            self.snapshot();
        }

        ui.separator();
        ui.heading("Presets");
        for row in 0..2 {
            ui.horizontal(|ui| {
                for slot in row * 4 + 1..=row * 4 + 4 {
                    if ui.button(format!("{slot}")).clicked() {
                        self.send(Command::Recall(slot));
                    }
                }
            });
        }
        ui.label("Save current position:");
        ui.horizontal(|ui| {
            for slot in 1..=8 {
                if ui.small_button(format!("{slot}")).clicked() {
                    self.send(Command::Save(slot));
                }
            }
        });

        if let Some(info) = &self.info {
            ui.separator();
            ui.label(info);
        }
        if let Some(message) = self.message.as_ref().or(self.worker_message.as_ref()) {
            ui.separator();
            ui.colored_label(egui::Color32::RED, message);
        }
    }
}

impl eframe::App for DesktopApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.sync_worker();
        let video_error = self.refresh_video(context);

        egui::TopBottomPanel::top("heading").show(context, |ui| {
            ui.horizontal(|ui| {
                ui.heading("OpenPTZ");
                ui.separator();
                ui.label(&self.status.card_name);
                ui.label(&self.status.device_path);
                ui.separator();
                match self.video.current_resolution() {
                    Some((width, height)) => ui.label(format!(
                        "Video: {width}×{height}, {:.1} FPS",
                        self.video_fps
                    )),
                    None => ui.label(format!("Video: {:.1} FPS", self.video_fps)),
                };
            });
        });
        egui::SidePanel::right("controls")
            .min_width(270.0)
            .show(context, |ui| self.controls(ui));
        egui::CentralPanel::default().show(context, |ui| {
            if let Some(texture) = &self.texture {
                let available = ui.available_size();
                let original = texture.size_vec2();
                let scale = (available.x / original.x)
                    .min(available.y / original.y)
                    .min(1.0);
                let size = Vec2::new(original.x * scale, original.y * scale);
                let mut aim = None;
                ui.centered_and_justified(|ui| {
                    let response =
                        ui.add(egui::Image::new((texture.id(), size)).sense(egui::Sense::click()));
                    if response.clicked() {
                        if let Some(pos) = response.interact_pointer_pos() {
                            let r = response.rect;
                            aim = Some((
                                (pos.x - r.left()) / r.width(),
                                (pos.y - r.top()) / r.height(),
                            ));
                        }
                    }
                });
                if let Some((x, y)) = aim {
                    self.send(Command::Aim(x, y));
                }
            } else if let Some(error) = video_error {
                ui.colored_label(egui::Color32::RED, format!("Video lost: {error}"));
            } else {
                ui.spinner();
                ui.label("Connecting to camera…");
            }
        });
        context.request_repaint_after(Duration::from_millis(16));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn center_click_is_zero() {
        let (p, t) = click_to_delta(0.5, 0.5, 1.0);
        assert!(p.abs() < 1e-6 && t.abs() < 1e-6);
    }

    #[test]
    fn edges_use_half_fov_and_zoom_narrows() {
        assert_eq!(click_to_delta(1.0, 0.0, 1.0), (20.0, 14.0));
        let (p, _) = click_to_delta(1.0, 0.5, 4.0);
        assert!((p - 20.0 / 2.8).abs() < 1e-4);
        assert_eq!(click_to_delta(5.0, -3.0, 1.0), (20.0, 14.0));
    }

    #[test]
    fn timestamp_format() {
        assert_eq!(timestamp_name(0), "19700101-000000");
        assert_eq!(timestamp_name(1_700_000_000), "20231114-221320");
    }
}
