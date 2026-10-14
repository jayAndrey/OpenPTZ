use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use tiny_http::{Header, Response, Server, StatusCode};

use crate::presets::PresetManager;
use crate::v4l2::CameraDevice;

pub struct ApiServer {
    device: Arc<Mutex<CameraDevice>>,
    preset_mgr: Arc<Mutex<PresetManager>>,
    port: u16,
}

impl ApiServer {
    pub fn new(device: CameraDevice, port: u16) -> Self {
        Self {
            device: Arc::new(Mutex::new(device)),
            preset_mgr: Arc::new(Mutex::new(PresetManager::load())),
            port,
        }
    }

    pub fn run(self) -> Result<()> {
        let addr = format!("0.0.0.0:{}", self.port);
        let server = Server::http(&addr).map_err(|e| anyhow::anyhow!("{}", e))
            .with_context(|| format!("Failed to bind HTTP server to {}", addr))?;

        println!("🌐 PTZ HTTP API Server listening on http://{}", addr);
        println!("Endpoints:");
        println!("  GET  /api/status");
        println!("  POST /api/pan?deg=10.0  or ?step=5.0 / ?dir=left|right");
        println!("  POST /api/tilt?deg=-5.0 or ?step=5.0 / ?dir=up|down");
        println!("  POST /api/zoom?x=2.0    or ?step=0.5 / ?dir=in|out");
        println!("  POST /api/home");
        println!("  POST /api/preset/recall/<1-8>");
        println!("  POST /api/preset/save/<1-8>");
        println!("  POST /api/flicker?hz=50|60|0");
        println!("  POST /api/brightness?val=128");

        for request in server.incoming_requests() {
            let dev = Arc::clone(&self.device);
            let pm = Arc::clone(&self.preset_mgr);

            let url = request.url().to_string();
            let method = request.method().as_str().to_string();

            let (path, query_str) = match url.split_once('?') {
                Some((p, q)) => (p.to_string(), q.to_string()),
                None => (url, String::new()),
            };

            let query_params: HashMap<String, String> = query_str
                .split('&')
                .filter_map(|pair| pair.split_once('='))
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();

            let response = Self::handle_request(&dev, &pm, &method, &path, &query_params);

            let json_header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
            let cors_header = Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap();

            let (status_code, body) = response;
            let resp = Response::from_string(body)
                .with_status_code(StatusCode(status_code))
                .with_header(json_header)
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
    ) -> (u16, String) {
        let camera = match dev.lock() {
            Ok(c) => c,
            Err(_) => return (500, r#"{"error":"Camera mutex poisoned"}"#.to_string()),
        };

        if method == "GET" && path == "/api/status" {
            let status = camera.get_status();
            return match serde_json::to_string_pretty(&status) {
                Ok(s) => (200, s),
                Err(e) => (500, format!(r#"{{"error":"{}"}}"#, e)),
            };
        }

        if method == "POST" || method == "GET" {
            match path {
                "/api/home" => {
                    return match camera.goto_home() {
                        Ok(()) => (200, r#"{"status":"ok","action":"home"}"#.to_string()),
                        Err(e) => (500, format!(r#"{{"error":"{}"}}"#, e)),
                    };
                }

                "/api/pan" => {
                    if let Some(deg_str) = params.get("deg") {
                        if let Ok(deg) = deg_str.parse::<f32>() {
                            return match camera.set_pan_deg(deg) {
                                Ok(()) => (200, format!(r#"{{"status":"ok","pan_deg":{}}}"#, deg)),
                                Err(e) => (500, format!(r#"{{"error":"{}"}}"#, e)),
                            };
                        }
                    }
                    if let Some(dir) = params.get("dir") {
                        let step = params.get("step").and_then(|s| s.parse::<f32>().ok()).unwrap_or(5.0);
                        let delta = if dir == "left" { -step } else { step };
                        return match camera.pan_relative(delta) {
                            Ok(new_pan) => (200, format!(r#"{{"status":"ok","pan_deg":{}}}"#, new_pan)),
                            Err(e) => (500, format!(r#"{{"error":"{}"}}"#, e)),
                        };
                    }
                }

                "/api/tilt" => {
                    if let Some(deg_str) = params.get("deg") {
                        if let Ok(deg) = deg_str.parse::<f32>() {
                            return match camera.set_tilt_deg(deg) {
                                Ok(()) => (200, format!(r#"{{"status":"ok","tilt_deg":{}}}"#, deg)),
                                Err(e) => (500, format!(r#"{{"error":"{}"}}"#, e)),
                            };
                        }
                    }
                    if let Some(dir) = params.get("dir") {
                        let step = params.get("step").and_then(|s| s.parse::<f32>().ok()).unwrap_or(5.0);
                        let delta = if dir == "down" { -step } else { step };
                        return match camera.tilt_relative(delta) {
                            Ok(new_tilt) => (200, format!(r#"{{"status":"ok","tilt_deg":{}}}"#, new_tilt)),
                            Err(e) => (500, format!(r#"{{"error":"{}"}}"#, e)),
                        };
                    }
                }

                "/api/zoom" => {
                    if let Some(x_str) = params.get("x") {
                        if let Ok(x) = x_str.parse::<f32>() {
                            return match camera.set_zoom_x(x) {
                                Ok(()) => (200, format!(r#"{{"status":"ok","zoom_x":{}}}"#, x)),
                                Err(e) => (500, format!(r#"{{"error":"{}"}}"#, e)),
                            };
                        }
                    }
                    if let Some(dir) = params.get("dir") {
                        let step = params.get("step").and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.5);
                        let delta = if dir == "out" { -step } else { step };
                        return match camera.zoom_relative(delta) {
                            Ok(new_zoom) => (200, format!(r#"{{"status":"ok","zoom_x":{}}}"#, new_zoom)),
                            Err(e) => (500, format!(r#"{{"error":"{}"}}"#, e)),
                        };
                    }
                }

                "/api/flicker" => {
                    if let Some(hz_str) = params.get("hz") {
                        if let Ok(hz) = hz_str.parse::<u32>() {
                            return match camera.set_power_line_frequency(hz) {
                                Ok(()) => (200, format!(r#"{{"status":"ok","power_line_frequency_hz":{}}}"#, hz)),
                                Err(e) => (400, format!(r#"{{"error":"{}"}}"#, e)),
                            };
                        }
                    }
                }

                "/api/brightness" => {
                    if let Some(val_str) = params.get("val") {
                        if let Ok(val) = val_str.parse::<i32>() {
                            return match camera.set_brightness(val) {
                                Ok(()) => (200, format!(r#"{{"status":"ok","brightness":{}}}"#, val)),
                                Err(e) => (500, format!(r#"{{"error":"{}"}}"#, e)),
                            };
                        }
                    }
                }

                _ => {}
            }

            // Hardware Presets pattern: /api/preset/recall/N or /api/preset/save/N
            if let Some(num_str) = path.strip_prefix("/api/preset/recall/") {
                if let Ok(num) = num_str.parse::<u8>() {
                    return match camera.goto_hardware_preset(num) {
                        Ok(()) => (200, format!(r#"{{"status":"ok","preset_recalled":{}}}"#, num)),
                        Err(e) => (500, format!(r#"{{"error":"{}"}}"#, e)),
                    };
                }
            }

            if let Some(num_str) = path.strip_prefix("/api/preset/save/") {
                if let Ok(num) = num_str.parse::<u8>() {
                    return match camera.save_hardware_preset(num) {
                        Ok(()) => (200, format!(r#"{{"status":"ok","preset_saved":{}}}"#, num)),
                        Err(e) => (500, format!(r#"{{"error":"{}"}}"#, e)),
                    };
                }
            }

            // Software Presets
            if path == "/api/software-preset/list" {
                if let Ok(mgr) = pm.lock() {
                    let list = mgr.list();
                    return match serde_json::to_string_pretty(&list) {
                        Ok(s) => (200, s),
                        Err(e) => (500, format!(r#"{{"error":"{}"}}"#, e)),
                    };
                }
            }
        }

        (404, r#"{"error":"Endpoint not found"}"#.to_string())
    }
}
