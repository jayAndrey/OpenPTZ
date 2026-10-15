use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use tiny_http::{Header, Response, Server, StatusCode};

use crate::presets::PresetManager;
use crate::v4l2::CameraDevice;
use crate::video::{MjpegReader, VideoHub};

const INDEX_HTML: &str = include_str!("web/index.html");

pub struct ApiServer {
    device: Arc<Mutex<CameraDevice>>,
    preset_mgr: Arc<Mutex<PresetManager>>,
    video: Arc<VideoHub>,
    port: u16,
}

impl ApiServer {
    pub fn new(device: CameraDevice, port: u16) -> Self {
        let status = device.get_status();
        Self {
            device: Arc::new(Mutex::new(device)),
            preset_mgr: Arc::new(Mutex::new(PresetManager::load())),
            video: VideoHub::start(status.device_path, status.card_name, false),
            port,
        }
    }

    pub fn run(self) -> Result<()> {
        let addr = format!("0.0.0.0:{}", self.port);
        let server = Server::http(&addr)
            .map_err(|e| anyhow::anyhow!("{}", e))
            .with_context(|| format!("Failed to bind HTTP server to {}", addr))?;

        println!(
            "🌐 PTZ Controller running at: http://localhost:{}",
            self.port
        );
        println!(
            "   (Also accessible from other devices at: http://{}:{})",
            get_local_ip().unwrap_or_else(|| "0.0.0.0".to_string()),
            self.port
        );

        for request in server.incoming_requests() {
            let dev = Arc::clone(&self.device);
            let pm = Arc::clone(&self.preset_mgr);

            let url = request.url().to_string();
            let method = request.method().as_str().to_string();

            let (path, query_str) = match url.split_once('?') {
                Some((p, q)) => (p.to_string(), q.to_string()),
                None => (url, String::new()),
            };

            if method == "GET" && path == "/api/video.mjpg" {
                let hub = Arc::clone(&self.video);
                std::thread::spawn(move || {
                    let content_type = Header::from_bytes(
                        &b"Content-Type"[..],
                        &b"multipart/x-mixed-replace; boundary=frame"[..],
                    )
                    .expect("static multipart content type");
                    let cache = Header::from_bytes(&b"Cache-Control"[..], &b"no-store"[..])
                        .expect("static cache header");
                    let response = Response::new(
                        StatusCode(200),
                        vec![content_type, cache],
                        MjpegReader::new(hub),
                        None,
                        None,
                    );
                    let _ = request.respond(response);
                });
                continue;
            }

            if method == "GET" && path == "/api/video/status" {
                let (_, frame, error) = self.video.latest();
                let (format, nominal_fps) = self.video.format_info();
                let body = serde_json::json!({
                    "ready": frame.is_some(),
                    "width": frame.as_ref().map(|f| f.width),
                    "height": frame.as_ref().map(|f| f.height),
                    "fps": self.video.fps(),
                    "format": format,
                    "nominal_fps": nominal_fps,
                    "error": error,
                });
                let response = Response::from_string(body.to_string()).with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                        .expect("static JSON content type"),
                );
                let _ = request.respond(response);
                continue;
            }

            let query_params: HashMap<String, String> = query_str
                .split('&')
                .filter_map(|pair| pair.split_once('='))
                .map(|(k, v)| {
                    let key = urlencoding_decode(k);
                    let val = urlencoding_decode(v);
                    (key, val)
                })
                .collect();

            let (status_code, content_type, body) =
                Self::handle_request(&dev, &pm, &method, &path, &query_params);

            let ct_header =
                Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()).unwrap();
            let cors_header =
                Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap();

            let resp = Response::from_string(body)
                .with_status_code(StatusCode(status_code))
                .with_header(ct_header)
                .with_header(cors_header);

            let _ = request.respond(resp);
        }

        Ok(())
    }

    fn handle_request(
        dev: &Arc<Mutex<CameraDevice>>,
        pm: &Arc<Mutex<PresetManager>>,
        method: &str,
        path: &str,
        params: &HashMap<String, String>,
    ) -> (u16, &'static str, String) {
        // Serve Web GUI
        if (method == "GET" || method == "HEAD") && (path == "/" || path == "/index.html") {
            return (200, "text/html; charset=utf-8", INDEX_HTML.to_string());
        }

        let camera = match dev.lock() {
            Ok(c) => c,
            Err(_) => {
                return (
                    500,
                    "application/json",
                    r#"{"error":"Camera mutex poisoned"}"#.to_string(),
                )
            }
        };

        if method == "GET" && path == "/api/status" {
            let status = camera.get_status();
            return match serde_json::to_string_pretty(&status) {
                Ok(s) => (200, "application/json", s),
                Err(e) => (500, "application/json", format!(r#"{{"error":"{}"}}"#, e)),
            };
        }

        // List presets
        if method == "GET" && (path == "/api/preset/list" || path == "/api/software-preset/list") {
            if let Ok(mgr) = pm.lock() {
                let list = mgr.list();
                return match serde_json::to_string_pretty(&list) {
                    Ok(s) => (200, "application/json", s),
                    Err(e) => (500, "application/json", format!(r#"{{"error":"{}"}}"#, e)),
                };
            }
        }

        if method == "POST" || method == "GET" {
            match path {
                "/api/log" => {
                    if let Some(msg) = params.get("msg") {
                        eprintln!("[CLIENT LOG] {}", msg);
                    }
                    return (200, "application/json", r#"{"status":"ok"}"#.to_string());
                }

                "/api/wake" => {
                    return match camera.wake_privacy() {
                        Ok(()) => (
                            200,
                            "application/json",
                            r#"{"status":"ok","action":"wake"}"#.to_string(),
                        ),
                        Err(e) => (500, "application/json", format!(r#"{{"error":"{}"}}"#, e)),
                    };
                }

                "/api/home" => {
                    return match camera.goto_home() {
                        Ok(()) => (
                            200,
                            "application/json",
                            r#"{"status":"ok","action":"home"}"#.to_string(),
                        ),
                        Err(e) => (500, "application/json", format!(r#"{{"error":"{}"}}"#, e)),
                    };
                }

                "/api/pan" => {
                    if let Some(deg_str) = params.get("deg") {
                        if let Ok(deg) = deg_str.parse::<f32>() {
                            return match camera.set_pan_deg(deg) {
                                Ok(()) => (
                                    200,
                                    "application/json",
                                    format!(r#"{{"status":"ok","pan_deg":{}}}"#, deg),
                                ),
                                Err(e) => {
                                    (500, "application/json", format!(r#"{{"error":"{}"}}"#, e))
                                }
                            };
                        }
                    }
                    if let Some(dir) = params.get("dir") {
                        let step = params
                            .get("step")
                            .and_then(|s| s.parse::<f32>().ok())
                            .unwrap_or(5.0);
                        let delta = if dir == "left" { -step } else { step };
                        return match camera.pan_relative(delta) {
                            Ok(new_pan) => (
                                200,
                                "application/json",
                                format!(r#"{{"status":"ok","pan_deg":{}}}"#, new_pan),
                            ),
                            Err(e) => (500, "application/json", format!(r#"{{"error":"{}"}}"#, e)),
                        };
                    }
                }

                "/api/tilt" => {
                    if let Some(deg_str) = params.get("deg") {
                        if let Ok(deg) = deg_str.parse::<f32>() {
                            return match camera.set_tilt_deg(deg) {
                                Ok(()) => (
                                    200,
                                    "application/json",
                                    format!(r#"{{"status":"ok","tilt_deg":{}}}"#, deg),
                                ),
                                Err(e) => {
                                    (500, "application/json", format!(r#"{{"error":"{}"}}"#, e))
                                }
                            };
                        }
                    }
                    if let Some(dir) = params.get("dir") {
                        let step = params
                            .get("step")
                            .and_then(|s| s.parse::<f32>().ok())
                            .unwrap_or(5.0);
                        let delta = if dir == "down" { -step } else { step };
                        return match camera.tilt_relative(delta) {
                            Ok(new_tilt) => (
                                200,
                                "application/json",
                                format!(r#"{{"status":"ok","tilt_deg":{}}}"#, new_tilt),
                            ),
                            Err(e) => (500, "application/json", format!(r#"{{"error":"{}"}}"#, e)),
                        };
                    }
                }

                "/api/zoom" => {
                    if let Some(x_str) = params.get("x") {
                        if let Ok(x) = x_str.parse::<f32>() {
                            return match camera.set_zoom_x(x) {
                                Ok(()) => (
                                    200,
                                    "application/json",
                                    format!(r#"{{"status":"ok","zoom_x":{}}}"#, x),
                                ),
                                Err(e) => {
                                    (500, "application/json", format!(r#"{{"error":"{}"}}"#, e))
                                }
                            };
                        }
                    }
                    if let Some(dir) = params.get("dir") {
                        let step = params
                            .get("step")
                            .and_then(|s| s.parse::<f32>().ok())
                            .unwrap_or(0.5);
                        let delta = if dir == "out" { -step } else { step };
                        return match camera.zoom_relative(delta) {
                            Ok(new_zoom) => (
                                200,
                                "application/json",
                                format!(r#"{{"status":"ok","zoom_x":{}}}"#, new_zoom),
                            ),
                            Err(e) => (500, "application/json", format!(r#"{{"error":"{}"}}"#, e)),
                        };
                    }
                }

                "/api/flicker" => {
                    if let Some(hz_str) = params.get("hz") {
                        if let Ok(hz) = hz_str.parse::<u32>() {
                            return match camera.set_power_line_frequency(hz) {
                                Ok(()) => (
                                    200,
                                    "application/json",
                                    format!(
                                        r#"{{"status":"ok","power_line_frequency_hz":{}}}"#,
                                        hz
                                    ),
                                ),
                                Err(e) => {
                                    (400, "application/json", format!(r#"{{"error":"{}"}}"#, e))
                                }
                            };
                        }
                    }
                }

                "/api/brightness" => {
                    if let Some(val_str) = params.get("val") {
                        if let Ok(val) = val_str.parse::<i32>() {
                            return match camera.set_brightness(val) {
                                Ok(()) => (
                                    200,
                                    "application/json",
                                    format!(r#"{{"status":"ok","brightness":{}}}"#, val),
                                ),
                                Err(e) => {
                                    (500, "application/json", format!(r#"{{"error":"{}"}}"#, e))
                                }
                            };
                        }
                    }
                }

                "/api/contrast" => {
                    if let Some(val_str) = params.get("val") {
                        if let Ok(val) = val_str.parse::<i32>() {
                            return match camera.set_contrast(val) {
                                Ok(()) => (
                                    200,
                                    "application/json",
                                    format!(r#"{{"status":"ok","contrast":{}}}"#, val),
                                ),
                                Err(e) => {
                                    (500, "application/json", format!(r#"{{"error":"{}"}}"#, e))
                                }
                            };
                        }
                    }
                }

                _ => {}
            }

            // Presets by slot: /api/preset/save/<slot>
            if let Some(num_str) = path.strip_prefix("/api/preset/save/") {
                if let Ok(num) = num_str.parse::<u8>() {
                    if let Ok(mut mgr) = pm.lock() {
                        return match mgr.save_slot(num, &camera) {
                            Ok(p) => (
                                200,
                                "application/json",
                                format!(
                                    r#"{{"status":"ok","preset":{},"pan_deg":{},"tilt_deg":{},"zoom_x":{}}}"#,
                                    num, p.pan_deg, p.tilt_deg, p.zoom_x
                                ),
                            ),
                            Err(e) => (
                                500,
                                "application/json",
                                format!(r#"{{"error":"Failed to save preset: {}"}}"#, e),
                            ),
                        };
                    }
                }
            }

            // Presets by slot: /api/preset/recall/<slot>
            if let Some(num_str) = path.strip_prefix("/api/preset/recall/") {
                if let Ok(num) = num_str.parse::<u8>() {
                    if let Ok(mgr) = pm.lock() {
                        return match mgr.recall_slot(num, &camera) {
                            Ok(p) => (
                                200,
                                "application/json",
                                format!(
                                    r#"{{"status":"ok","preset":{},"pan_deg":{},"tilt_deg":{},"zoom_x":{}}}"#,
                                    num, p.pan_deg, p.tilt_deg, p.zoom_x
                                ),
                            ),
                            Err(e) => (404, "application/json", format!(r#"{{"error":"{}"}}"#, e)),
                        };
                    }
                }
            }

            // Named Presets: save
            if path == "/api/preset/save" || path == "/api/software-preset/save" {
                if let Some(name) = params.get("name") {
                    let pan = camera.get_pan_deg().unwrap_or(0.0);
                    let tilt = camera.get_tilt_deg().unwrap_or(0.0);
                    let zoom = camera.get_zoom_x().unwrap_or(1.0);
                    let b = camera.get_brightness().ok();
                    let f = camera.get_power_line_frequency().ok();

                    if let Ok(mut mgr) = pm.lock() {
                        return match mgr.set_preset(name, pan, tilt, zoom, b, f) {
                            Ok(p) => (
                                200,
                                "application/json",
                                format!(
                                    r#"{{"status":"ok","saved":"{}","pan_deg":{},"tilt_deg":{},"zoom_x":{}}}"#,
                                    name, p.pan_deg, p.tilt_deg, p.zoom_x
                                ),
                            ),
                            Err(e) => (500, "application/json", format!(r#"{{"error":"{}"}}"#, e)),
                        };
                    }
                }
            }

            // Named Presets: recall
            if path == "/api/preset/recall" || path == "/api/software-preset/recall" {
                if let Some(name) = params.get("name") {
                    if let Ok(mgr) = pm.lock() {
                        return match mgr.apply(&camera, name) {
                            Ok(p) => (
                                200,
                                "application/json",
                                format!(
                                    r#"{{"status":"ok","recalled":"{}","pan_deg":{},"tilt_deg":{},"zoom_x":{}}}"#,
                                    name, p.pan_deg, p.tilt_deg, p.zoom_x
                                ),
                            ),
                            Err(e) => (404, "application/json", format!(r#"{{"error":"{}"}}"#, e)),
                        };
                    }
                }
            }

            // Named Presets: delete
            if path == "/api/preset/delete" || path == "/api/software-preset/delete" {
                if let Some(name) = params.get("name") {
                    if let Ok(mut mgr) = pm.lock() {
                        let _ = mgr.delete_preset(name);
                        return (
                            200,
                            "application/json",
                            format!(r#"{{"status":"ok","deleted":"{}"}}"#, name),
                        );
                    }
                }
            }
        }

        (
            404,
            "application/json",
            r#"{"error":"Endpoint not found"}"#.to_string(),
        )
    }
}

fn urlencoding_decode(val: &str) -> String {
    let mut result = String::new();
    let mut chars = val.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let h1 = chars.next();
            let h2 = chars.next();
            if let (Some(c1), Some(c2)) = (h1, h2) {
                let hex_str = format!("{}{}", c1, c2);
                if let Ok(byte) = u8::from_str_radix(&hex_str, 16) {
                    result.push(byte as char);
                    continue;
                }
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    result
}

fn get_local_ip() -> Option<String> {
    use std::net::UdpSocket;
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}
