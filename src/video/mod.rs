use std::io::{self, Read};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use image::codecs::jpeg::JpegEncoder;
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{ApiBackend, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType};
use nokhwa::{query, Camera};

#[derive(Clone)]
pub struct VideoFrame {
    pub width: usize,
    pub height: usize,
    pub rgb: Option<Arc<[u8]>>,
    pub jpeg: Arc<[u8]>,
}

#[derive(Default)]
struct CaptureState {
    sequence: u64,
    frame: Option<Arc<VideoFrame>>,
    error: Option<String>,
    fps: f32,
    format: String,
    nominal_fps: u32,
    stopped: bool,
}

pub struct VideoHub {
    state: Mutex<CaptureState>,
    changed: Condvar,
    thread: Mutex<Option<JoinHandle<()>>>,
}

const RETRY_INITIAL: Duration = Duration::from_secs(1);
const RETRY_MAX: Duration = Duration::from_secs(5);

/// Delay before reconnect attempt `attempt` (0-based): 1s, 2s, 4s, then capped at 5s.
fn retry_delay(attempt: u32) -> Duration {
    RETRY_INITIAL
        .checked_mul(1u32.checked_shl(attempt).unwrap_or(u32::MAX))
        .map_or(RETRY_MAX, |delay| delay.min(RETRY_MAX))
}

/// Lists available video sources as `(human name, index string)` pairs.
impl VideoHub {
    fn idle() -> Self {
        Self {
            state: Mutex::new(CaptureState::default()),
            changed: Condvar::new(),
            thread: Mutex::new(None),
        }
    }

    pub fn start(device_path: String, camera_name: String, need_rgb: bool) -> Arc<Self> {
        Self::start_with(device_path, camera_name, need_rgb, None)
    }

    pub fn start_with(
        device_path: String,
        camera_name: String,
        need_rgb: bool,
        resolution: Option<(u32, u32)>,
    ) -> Arc<Self> {
        let hub = Arc::new(Self::idle());
        let capture_hub = Arc::clone(&hub);
        let handle = std::thread::spawn(move || {
            capture_hub.run(&device_path, &camera_name, need_rgb, resolution);
        });
        *hub.thread.lock().unwrap_or_else(|e| e.into_inner()) = Some(handle);
        hub
    }

    /// Stops the capture thread and wakes any waiting readers.
    pub fn stop(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.stopped = true;
        self.changed.notify_all();
    }

    /// Stops the capture thread and waits for it to release the camera.
    /// Call from a worker thread, not the UI thread: it blocks until the
    /// capture thread notices the stop flag (at most one frame/read timeout).
    pub fn stop_and_join(&self) {
        self.stop();
        let handle = self.thread.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(handle) = handle {
            if handle.thread().id() != std::thread::current().id() {
                let _ = handle.join();
            }
        }
    }

    pub fn latest(&self) -> (u64, Option<Arc<VideoFrame>>, Option<String>) {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        (state.sequence, state.frame.clone(), state.error.clone())
    }

    pub fn fps(&self) -> f32 {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).fps
    }

    pub fn format_info(&self) -> (String, u32) {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        (state.format.clone(), state.nominal_fps)
    }

    /// Resolution `(width, height)` of the latest frame, if any.
    pub fn current_resolution(&self) -> Option<(usize, usize)> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.frame.as_ref().map(|f| (f.width, f.height))
    }

    fn is_stopped(&self) -> bool {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).stopped
    }

    fn set_error(&self, message: String) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.error = Some(message);
        state.frame = None;
        self.changed.notify_all();
    }

    /// Sleeps for `delay`, returning false early if the hub was stopped.
    fn sleep_unless_stopped(&self, delay: Duration) -> bool {
        let deadline = Instant::now() + delay;
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        while !state.stopped {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return true;
            };
            state = self
                .changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        false
    }

    fn run(
        &self,
        device_path: &str,
        camera_name: &str,
        need_rgb: bool,
        resolution: Option<(u32, u32)>,
    ) {
        let mut attempt = 0u32;
        while !self.is_stopped() {
            let mut got_frame = false;
            match self.capture(
                device_path,
                camera_name,
                need_rgb,
                resolution,
                &mut got_frame,
            ) {
                Ok(()) => break,
                Err(error) => {
                    if got_frame {
                        attempt = 0;
                    }
                    self.set_error(format!("Camera lost: {error:#}"));
                    let delay = retry_delay(attempt);
                    attempt = attempt.saturating_add(1);
                    if !self.sleep_unless_stopped(delay) {
                        break;
                    }
                }
            }
        }
    }

    /// Runs one capture session; returns `Ok` only when stopped.
    fn capture(
        &self,
        device_path: &str,
        camera_name: &str,
        need_rgb: bool,
        resolution: Option<(u32, u32)>,
        got_frame: &mut bool,
    ) -> Result<()> {
        let index = select_camera(device_path, camera_name)?;
        let mut camera = open_camera(index, resolution)?;
        println!(
            "Video capture configured: {} (RGB decode: {})",
            camera.camera_format(),
            need_rgb
        );
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.format = camera.frame_format().to_string();
            state.nominal_fps = camera.frame_rate();
        }
        camera
            .open_stream()
            .context("Could not start camera stream")?;

        let mut fps_started = Instant::now();
        let mut fps_frames = 0u32;
        while !self.is_stopped() {
            let frame = camera.frame().context("Could not read camera frame")?;
            let resolution = frame.resolution();
            let needs_decode = need_rgb || frame.source_frame_format() != FrameFormat::MJPEG;
            let rgb = if needs_decode {
                Some(
                    frame
                        .decode_image::<RgbFormat>()
                        .context("Could not decode camera frame")?
                        .into_raw(),
                )
            } else {
                None
            };
            let width = resolution.width_x as usize;
            let height = resolution.height_y as usize;
            let jpeg: Arc<[u8]> = if frame.source_frame_format() == FrameFormat::MJPEG {
                Arc::from(frame.buffer())
            } else {
                let mut jpeg = Vec::new();
                JpegEncoder::new_with_quality(&mut jpeg, 80)
                    .encode(
                        rgb.as_deref().expect("non-MJPEG frames are decoded"),
                        width as u32,
                        height as u32,
                        image::ExtendedColorType::Rgb8,
                    )
                    .context("Could not encode camera frame")?;
                jpeg.into()
            };

            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if state.stopped {
                break;
            }
            *got_frame = true;
            state.sequence = state.sequence.wrapping_add(1);
            fps_frames += 1;
            let elapsed = fps_started.elapsed();
            if elapsed >= Duration::from_secs(1) {
                state.fps = fps_frames as f32 / elapsed.as_secs_f32();
                fps_frames = 0;
                fps_started = Instant::now();
            }
            state.frame = Some(Arc::new(VideoFrame {
                width,
                height,
                rgb: rgb.map(Into::into),
                jpeg,
            }));
            state.error = None;
            self.changed.notify_all();
            drop(state);
        }
        Ok(())
    }

    fn wait_next(&self, after: u64) -> Option<Arc<VideoFrame>> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if state.stopped {
                return None;
            }
            if state.sequence != after {
                if let Some(frame) = &state.frame {
                    return Some(Arc::clone(frame));
                }
            }
            state = self
                .changed
                .wait_timeout(state, Duration::from_millis(250))
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }
}

fn select_camera(device_path: &str, camera_name: &str) -> Result<CameraIndex> {
    let cameras = query(ApiBackend::Auto).context("Could not enumerate video cameras")?;
    let path = device_path
        .to_lowercase()
        .trim_start_matches("@device:pnp:")
        .to_string();
    let video_index = path
        .strip_prefix("/dev/video")
        .and_then(|index| index.parse::<u32>().ok());
    let path_matches: Vec<_> = cameras
        .iter()
        .filter(|info| {
            let index = info.index().as_string().to_lowercase();
            let misc = info.misc().to_lowercase();
            index == path
                || video_index
                    .is_some_and(|expected| info.index().as_index().ok() == Some(expected))
                || misc == path
                || (!path.is_empty()
                    && !misc.is_empty()
                    && (misc.contains(&path) || path.contains(&misc)))
        })
        .collect();
    if path_matches.len() == 1 {
        return Ok(path_matches[0].index().clone());
    }

    let name_matches: Vec<_> = cameras
        .iter()
        .filter(|info| info.human_name().eq_ignore_ascii_case(camera_name))
        .collect();
    if name_matches.len() == 1 {
        return Ok(name_matches[0].index().clone());
    }
    Err(anyhow!(
        "Could not uniquely match PTZ camera '{camera_name}' ({device_path}) to a video source"
    ))
}

fn open_camera(index: CameraIndex, preferred: Option<(u32, u32)>) -> Result<Camera> {
    const MJPEG: &[FrameFormat] = &[FrameFormat::MJPEG];
    const RGB_FORMATS: &[FrameFormat] = &[
        FrameFormat::MJPEG,
        FrameFormat::YUYV,
        FrameFormat::NV12,
        FrameFormat::RAWRGB,
        FrameFormat::RAWBGR,
    ];
    let resolutions: Vec<(u32, u32)> = preferred
        .into_iter()
        .chain([(1280, 720), (1920, 1080)])
        .collect();
    let requests = resolutions
        .into_iter()
        .flat_map(|(width, height)| {
            let resolution = nokhwa::utils::Resolution::new(width, height);
            [
                (RequestedFormatType::HighestResolution(resolution), MJPEG),
                (
                    RequestedFormatType::HighestResolution(resolution),
                    RGB_FORMATS,
                ),
            ]
        })
        .chain(std::iter::once((RequestedFormatType::None, RGB_FORMATS)));
    let mut last_error = None;
    for (requested, formats) in requests {
        let request = RequestedFormat::with_formats(requested, formats);
        match Camera::new(index.clone(), request) {
            Ok(camera) => return Ok(camera),
            Err(error) => last_error = Some(error),
        }
    }
    Err(anyhow!("Could not open video device: {last_error:?}"))
}

pub struct MjpegReader {
    hub: Arc<VideoHub>,
    sequence: u64,
    pending: Vec<u8>,
    offset: usize,
}

impl MjpegReader {
    pub fn new(hub: Arc<VideoHub>) -> Self {
        Self {
            hub,
            sequence: 0,
            pending: Vec::new(),
            offset: 0,
        }
    }
}

impl Read for MjpegReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        if self.offset == self.pending.len() {
            let frame = match self.hub.wait_next(self.sequence) {
                Some(frame) => frame,
                None => return Ok(0),
            };
            self.sequence = self.hub.latest().0;
            self.pending = format!(
                "--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
                frame.jpeg.len()
            )
            .into_bytes();
            self.pending.extend_from_slice(&frame.jpeg);
            self.pending.extend_from_slice(b"\r\n");
            self.offset = 0;
        }
        let count = output.len().min(self.pending.len() - self.offset);
        output[..count].copy_from_slice(&self.pending[self.offset..self.offset + count]);
        self.offset += count;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::{retry_delay, CaptureState, MjpegReader, VideoFrame, VideoHub};
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::Duration;

    #[test]
    fn mjpeg_reader_sends_complete_jpeg_part() {
        let hub = Arc::new(VideoHub {
            state: Mutex::new(CaptureState {
                sequence: 1,
                frame: Some(Arc::new(VideoFrame {
                    width: 1,
                    height: 1,
                    rgb: Some(Arc::from([0u8, 0, 0])),
                    jpeg: Arc::from([0xffu8, 0xd8, 0xff, 0xd9]),
                })),
                error: None,
                fps: 0.0,
                format: "MJPEG".to_string(),
                nominal_fps: 30,
                stopped: false,
            }),
            changed: Condvar::new(),
            thread: Mutex::new(None),
        });
        let mut reader = MjpegReader::new(hub);
        let mut data = vec![0; 100];
        let read = reader.read(&mut data).unwrap();
        assert!(data[..read].starts_with(b"--frame\r\nContent-Type: image/jpeg\r\n"));
        assert!(data[..read].ends_with(&[0xff, 0xd8, 0xff, 0xd9, b'\r', b'\n']));
    }

    #[test]
    fn retry_delay_backs_off_and_caps() {
        let secs: Vec<u64> = (0..6).map(|n| retry_delay(n).as_secs()).collect();
        assert_eq!(secs, [1, 2, 4, 5, 5, 5]);
        assert_eq!(retry_delay(u32::MAX), Duration::from_secs(5));
    }

    #[test]
    fn stop_wakes_wait_next() {
        let hub = Arc::new(VideoHub::idle());
        let waiter = Arc::clone(&hub);
        let handle = std::thread::spawn(move || waiter.wait_next(0));
        std::thread::sleep(Duration::from_millis(50));
        hub.stop();
        assert!(handle.join().unwrap().is_none());
        assert!(!hub.sleep_unless_stopped(Duration::from_secs(10)));
        assert_eq!(hub.current_resolution(), None);
    }

    #[test]
    fn wait_next_survives_errors_until_new_frame() {
        let hub = Arc::new(VideoHub::idle());
        hub.set_error("Camera lost: test".into());
        let waiter = Arc::clone(&hub);
        let handle = std::thread::spawn(move || waiter.wait_next(0));
        std::thread::sleep(Duration::from_millis(100));
        assert!(!handle.is_finished());
        {
            let mut state = hub.state.lock().unwrap();
            state.sequence = 1;
            state.error = None;
            state.frame = Some(Arc::new(VideoFrame {
                width: 1,
                height: 1,
                rgb: None,
                jpeg: Arc::from([0xffu8, 0xd8, 0xff, 0xd9]),
            }));
        }
        hub.changed.notify_all();
        assert!(handle.join().unwrap().is_some());
    }

    #[test]
    fn stop_and_join_waits_for_thread() {
        let hub = Arc::new(VideoHub::idle());
        let worker = Arc::clone(&hub);
        let handle = std::thread::spawn(move || {
            worker.sleep_unless_stopped(Duration::from_secs(30));
        });
        *hub.thread.lock().unwrap() = Some(handle);
        hub.stop_and_join();
        assert!(hub.thread.lock().unwrap().is_none());
        assert!(hub.is_stopped());
    }
}
