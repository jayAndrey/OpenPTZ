use std::io::stdout;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{canvas::Canvas, Block, BorderType, Borders, Gauge, Paragraph, Wrap},
    Frame, Terminal,
};

use crate::presets::PresetManager;
use crate::v4l2::CameraDevice;

pub struct TuiApp {
    device: CameraDevice,
    preset_mgr: PresetManager,
    step_deg: f32,
    zoom_step: f32,
    status_message: String,
    status_time: Instant,
    should_quit: bool,
}

impl TuiApp {
    pub fn new(device: CameraDevice) -> Self {
        Self {
            device,
            preset_mgr: PresetManager::load(),
            step_deg: 5.0,
            zoom_step: 0.5,
            status_message: "Ready. Use Arrow keys to Pan/Tilt, +/- to Zoom, 1-8 for Presets."
                .to_string(),
            status_time: Instant::now(),
            should_quit: false,
        }
    }

    pub fn set_status<S: Into<String>>(&mut self, msg: S) {
        self.status_message = msg.into();
        self.status_time = Instant::now();
    }

    pub fn run(&mut self) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let tick_rate = Duration::from_millis(50);
        let mut last_tick = Instant::now();

        while !self.should_quit {
            terminal.draw(|f| self.ui(f))?;

            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));

            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.handle_key(key.code, key.modifiers);
                    }
                }
            }

            if last_tick.elapsed() >= tick_rate {
                last_tick = Instant::now();
            }
        }

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;
        Ok(())
    }

    fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        match (code, modifiers) {
            // Quit
            (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => {
                self.should_quit = true;
            }
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }

            // Pan / Tilt Movements
            (KeyCode::Left, _) => match self.device.pan_relative(-self.step_deg) {
                Ok(pos) => self.set_status(format!("Pan Left: {:.1}°", pos)),
                Err(e) => self.set_status(format!("Error: {}", e)),
            },
            (KeyCode::Right, _) => match self.device.pan_relative(self.step_deg) {
                Ok(pos) => self.set_status(format!("Pan Right: {:.1}°", pos)),
                Err(e) => self.set_status(format!("Error: {}", e)),
            },
            (KeyCode::Up, _) => match self.device.tilt_relative(self.step_deg) {
                Ok(pos) => self.set_status(format!("Tilt Up: {:.1}°", pos)),
                Err(e) => self.set_status(format!("Error: {}", e)),
            },
            (KeyCode::Down, _) => match self.device.tilt_relative(-self.step_deg) {
                Ok(pos) => self.set_status(format!("Tilt Down: {:.1}°", pos)),
                Err(e) => self.set_status(format!("Error: {}", e)),
            },

            // Zoom
            (KeyCode::Char('+'), _) | (KeyCode::Char('='), _) | (KeyCode::PageUp, _) => {
                match self.device.zoom_relative(self.zoom_step) {
                    Ok(z) => self.set_status(format!("Zoom In: {:.1}x", z)),
                    Err(e) => self.set_status(format!("Error: {}", e)),
                }
            }
            (KeyCode::Char('-'), _) | (KeyCode::Char('_'), _) | (KeyCode::PageDown, _) => {
                match self.device.zoom_relative(-self.zoom_step) {
                    Ok(z) => self.set_status(format!("Zoom Out: {:.1}x", z)),
                    Err(e) => self.set_status(format!("Error: {}", e)),
                }
            }

            // Home Position
            (KeyCode::Home, _)
            | (KeyCode::Char('h'), KeyModifiers::NONE)
            | (KeyCode::Char('0'), _) => match self.device.goto_home() {
                Ok(()) => self.set_status("Camera returned to Home position."),
                Err(e) => self.set_status(format!("Home failed: {}", e)),
            },

            // Presets 1-8: Shift = Save, Normal = Recall
            (KeyCode::Char(c @ '1'..='8'), KeyModifiers::SHIFT) => {
                let num = c.to_digit(10).unwrap() as u8;
                match self.preset_mgr.save_slot(num, &self.device) {
                    Ok(p) => self.set_status(format!(
                        "Preset {} saved! (Pan={:+.1}°, Tilt={:+.1}°, Zoom={:.1}x)",
                        num, p.pan_deg, p.tilt_deg, p.zoom_x
                    )),
                    Err(e) => self.set_status(format!("Save Preset {} failed: {}", num, e)),
                }
            }
            (KeyCode::Char(c @ '1'..='8'), _) => {
                let num = c.to_digit(10).unwrap() as u8;
                match self.preset_mgr.recall_slot(num, &self.device) {
                    Ok(p) => self.set_status(format!(
                        "Moved to Preset {}: Pan={:+.1}°, Tilt={:+.1}°, Zoom={:.1}x",
                        num, p.pan_deg, p.tilt_deg, p.zoom_x
                    )),
                    Err(_) => self.set_status(format!(
                        "Preset {} is empty! Press Shift+{} to save current position.",
                        num, num
                    )),
                }
            }

            // Anti-Flicker / Power Line Frequency (PDF feature)
            (KeyCode::Char('f'), _) | (KeyCode::Char('F'), _) => {
                match self.device.cycle_power_line_frequency() {
                    Ok(val) => {
                        let text = match val {
                            1 => "50 Hz (Europe/Asia)",
                            2 => "60 Hz (Americas)",
                            _ => "Disabled",
                        };
                        self.set_status(format!("Anti-Flicker set to: {}", text));
                    }
                    Err(e) => self.set_status(format!("Failed to change anti-flicker: {}", e)),
                }
            }

            // Step Size adjustment
            (KeyCode::Char(']'), _) => {
                self.step_deg = match self.step_deg as i32 {
                    1 => 2.0,
                    2 => 5.0,
                    5 => 10.0,
                    10 => 20.0,
                    _ => 30.0,
                };
                self.set_status(format!("Step size increased to {:.0}°", self.step_deg));
            }
            (KeyCode::Char('['), _) => {
                self.step_deg = match self.step_deg as i32 {
                    30 => 20.0,
                    20 => 10.0,
                    10 => 5.0,
                    5 => 2.0,
                    _ => 1.0,
                };
                self.set_status(format!("Step size decreased to {:.0}°", self.step_deg));
            }

            // Brightness
            (KeyCode::Char('b'), KeyModifiers::NONE) => {
                let cur = self.device.get_brightness().unwrap_or(128);
                let _ = self.device.set_brightness((cur - 10).max(0));
                self.set_status(format!("Brightness: {}", (cur - 10).max(0)));
            }
            (KeyCode::Char('B'), _) => {
                let cur = self.device.get_brightness().unwrap_or(128);
                let _ = self.device.set_brightness((cur + 10).min(255));
                self.set_status(format!("Brightness: {}", (cur + 10).min(255)));
            }

            // Contrast
            (KeyCode::Char('c'), KeyModifiers::NONE) => {
                let cur = self.device.get_contrast().unwrap_or(128);
                let _ = self.device.set_contrast((cur - 10).max(0));
                self.set_status(format!("Contrast: {}", (cur - 10).max(0)));
            }
            (KeyCode::Char('C'), _) => {
                let cur = self.device.get_contrast().unwrap_or(128);
                let _ = self.device.set_contrast((cur + 10).min(255));
                self.set_status(format!("Contrast: {}", (cur + 10).min(255)));
            }

            _ => {}
        }
    }

    fn ui(&self, f: &mut Frame) {
        let size = f.area();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(12),   // Body
                Constraint::Length(3), // Status Bar
                Constraint::Length(2), // Keybindings
            ])
            .split(size);

        self.render_header(f, chunks[0]);
        self.render_body(f, chunks[1]);
        self.render_status(f, chunks[2]);
        self.render_keybindings(f, chunks[3]);
    }

    fn render_header(&self, f: &mut Frame, area: Rect) {
        let card = self.device.card_name();
        let path = self.device.path().to_string_lossy();
        let xu_badge = if self.device.has_rally_xu() {
            Span::styled(
                " [Logi XU: ACTIVE] ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(
                " [Logi XU: Standard V4L2] ",
                Style::default().fg(Color::Yellow),
            )
        };

        let title = Line::from(vec![
            Span::styled(
                "🎥 PTZ Control - Logitech Rally Camera ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            xu_badge,
            Span::raw(format!("  Device: {} ({})", card, path)),
        ]);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        let paragraph = Paragraph::new(title)
            .block(block)
            .alignment(Alignment::Left);
        f.render_widget(paragraph, area);
    }

    fn render_body(&self, f: &mut Frame, area: Rect) {
        let body_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(area);

        self.render_ptz_panel(f, body_chunks[0]);
        self.render_settings_panel(f, body_chunks[1]);
    }

    fn render_ptz_panel(&self, f: &mut Frame, area: Rect) {
        let pan_deg = self.device.get_pan_deg().unwrap_or(0.0);
        let tilt_deg = self.device.get_tilt_deg().unwrap_or(0.0);
        let zoom_x = self.device.get_zoom_x().unwrap_or(1.0);

        let ptz_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(8),    // 2D Canvas Target
                Constraint::Length(2), // Pan Gauge
                Constraint::Length(2), // Tilt Gauge
                Constraint::Length(2), // Zoom Gauge
            ])
            .split(area);

        // 2D Reticle Canvas
        let canvas = Canvas::default()
            .block(
                Block::default()
                    .title(" 🎯 Pan & Tilt Reticle (-90° to +90° Pan, -90° to +50° Tilt) ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Blue)),
            )
            .x_bounds([-90.0, 90.0])
            .y_bounds([-90.0, 50.0])
            .paint(move |ctx| {
                // Draw coordinate axes
                ctx.draw(&ratatui::widgets::canvas::Line {
                    x1: -90.0,
                    y1: 0.0,
                    x2: 90.0,
                    y2: 0.0,
                    color: Color::DarkGray,
                });
                ctx.draw(&ratatui::widgets::canvas::Line {
                    x1: 0.0,
                    y1: -90.0,
                    x2: 0.0,
                    y2: 50.0,
                    color: Color::DarkGray,
                });

                // Draw home marker at (0, 0)
                ctx.print(
                    0.0,
                    0.0,
                    Span::styled("⌂", Style::default().fg(Color::Yellow)),
                );

                // Draw current camera target crosshair
                ctx.draw(&ratatui::widgets::canvas::Line {
                    x1: (pan_deg - 8.0) as f64,
                    y1: tilt_deg as f64,
                    x2: (pan_deg + 8.0) as f64,
                    y2: tilt_deg as f64,
                    color: Color::Green,
                });
                ctx.draw(&ratatui::widgets::canvas::Line {
                    x1: pan_deg as f64,
                    y1: (tilt_deg - 6.0) as f64,
                    x2: pan_deg as f64,
                    y2: (tilt_deg + 6.0) as f64,
                    color: Color::Green,
                });
                ctx.print(
                    pan_deg as f64,
                    tilt_deg as f64,
                    Span::styled("◉", Style::default().fg(Color::Red)),
                );
            });

        f.render_widget(canvas, ptz_chunks[0]);

        // Pan Gauge (-90 to +90) -> (0% to 100%)
        let pan_pct = (((pan_deg + 90.0) / 180.0) * 100.0).clamp(0.0, 100.0) as u16;
        let pan_gauge = Gauge::default()
            .gauge_style(Style::default().fg(Color::Yellow).bg(Color::DarkGray))
            .percent(pan_pct)
            .label(format!(
                "Pan: {:+.1}° (Step: {:.0}°)",
                pan_deg, self.step_deg
            ));
        f.render_widget(pan_gauge, ptz_chunks[1]);

        // Tilt Gauge (-90 to +50) -> (0% to 100%)
        let tilt_pct = (((tilt_deg + 90.0) / 140.0) * 100.0).clamp(0.0, 100.0) as u16;
        let tilt_gauge = Gauge::default()
            .gauge_style(Style::default().fg(Color::Magenta).bg(Color::DarkGray))
            .percent(tilt_pct)
            .label(format!(
                "Tilt: {:+.1}° (Step: {:.0}°)",
                tilt_deg, self.step_deg
            ));
        f.render_widget(tilt_gauge, ptz_chunks[2]);

        // Zoom Gauge (1.0x to 15.0x)
        let zoom_pct = (((zoom_x - 1.0) / 14.0) * 100.0).clamp(0.0, 100.0) as u16;
        let zoom_gauge = Gauge::default()
            .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray))
            .percent(zoom_pct)
            .label(format!("Zoom: {:.1}x (Range 1.0x - 15.0x)", zoom_x));
        f.render_widget(zoom_gauge, ptz_chunks[3]);
    }

    fn render_settings_panel(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(area);

        // Top: Image Settings & Sensor
        let flicker_val = self.device.get_power_line_frequency().unwrap_or(0);
        let flicker_str = match flicker_val {
            1 => "50 Hz (PAL / Europe)",
            2 => "60 Hz (NTSC / Americas)",
            _ => "Disabled (0)",
        };

        let brightness = self.device.get_brightness().unwrap_or(128);
        let contrast = self.device.get_contrast().unwrap_or(128);
        let auto_exp = self.device.get_auto_exposure().unwrap_or(3);
        let exp_str = if auto_exp == 3 {
            "Auto (Aperture)"
        } else {
            "Manual"
        };
        let auto_focus = self.device.get_auto_focus().unwrap_or(true);

        let lines = vec![
            Line::from(vec![
                Span::styled(
                    "Anti-Flicker:   ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<20}", flicker_str),
                    Style::default().fg(Color::White),
                ),
                Span::styled(" [F to cycle]", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("Brightness:     ", Style::default().fg(Color::Yellow)),
                Span::raw(format!("{:<20}", brightness)),
                Span::styled(" [B / b]", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("Contrast:       ", Style::default().fg(Color::Yellow)),
                Span::raw(format!("{:<20}", contrast)),
                Span::styled(" [C / c]", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("Exposure Mode:  ", Style::default().fg(Color::Yellow)),
                Span::raw(format!("{:<20}", exp_str)),
            ]),
            Line::from(vec![
                Span::styled("Auto Focus:     ", Style::default().fg(Color::Yellow)),
                Span::raw(if auto_focus { "Enabled" } else { "Disabled" }),
            ]),
            Line::from(vec![
                Span::styled("Move Step Size: ", Style::default().fg(Color::Yellow)),
                Span::raw(format!("{:.0}°", self.step_deg)),
                Span::styled(
                    "             [ [ / ] ]",
                    Style::default().fg(Color::DarkGray),
                ),
            ]),
        ];

        let block = Block::default()
            .title(" ⚙️ Camera Settings & Anti-Flicker ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Yellow));

        let p = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false });
        f.render_widget(p, chunks[0]);

        // Bottom: Hardware Presets 1-8
        let preset_lines = vec![
            Line::from(vec![Span::styled(
                "Hardware Presets (Logitech EEPROM):",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  [1] Preset 1    ", Style::default().fg(Color::Cyan)),
                Span::styled("[2] Preset 2    ", Style::default().fg(Color::Cyan)),
                Span::styled("[3] Preset 3    ", Style::default().fg(Color::Cyan)),
                Span::styled("[4] Preset 4", Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("  [5] Preset 5    ", Style::default().fg(Color::Cyan)),
                Span::styled("[6] Preset 6    ", Style::default().fg(Color::Cyan)),
                Span::styled("[7] Preset 7    ", Style::default().fg(Color::Cyan)),
                Span::styled("[8] Preset 8", Style::default().fg(Color::Cyan)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Tip: ", Style::default().fg(Color::Yellow)),
                Span::raw("Press "),
                Span::styled(
                    "1-8",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" to Recall, "),
                Span::styled(
                    "Shift+1..8",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" to Save."),
            ]),
        ];

        let preset_block = Block::default()
            .title(" 💾 Presets ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Green));

        let preset_p = Paragraph::new(preset_lines).block(preset_block);
        f.render_widget(preset_p, chunks[1]);
    }

    fn render_status(&self, f: &mut Frame, area: Rect) {
        let elapsed = self.status_time.elapsed().as_secs();
        let color =
            if self.status_message.starts_with("Error") || self.status_message.contains("failed") {
                Color::Red
            } else if elapsed < 3 {
                Color::LightGreen
            } else {
                Color::White
            };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(color));

        let p = Paragraph::new(Span::styled(
            &self.status_message,
            Style::default().fg(color),
        ))
        .block(block)
        .alignment(Alignment::Left);

        f.render_widget(p, area);
    }

    fn render_keybindings(&self, f: &mut Frame, area: Rect) {
        let keys = Line::from(vec![
            Span::styled(
                "Arrows",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Pan/Tilt | "),
            Span::styled(
                "+/-",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Zoom | "),
            Span::styled(
                "Home/h",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Home | "),
            Span::styled(
                "1-8",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Recall | "),
            Span::styled(
                "Shift+1-8",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Save | "),
            Span::styled(
                "F",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Flicker | "),
            Span::styled(
                "[ / ]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Step | "),
            Span::styled(
                "Q/Esc",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Quit"),
        ]);

        let p = Paragraph::new(keys).alignment(Alignment::Center);
        f.render_widget(p, area);
    }
}
