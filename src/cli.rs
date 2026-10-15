use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "openptz",
    author = "andryjay",
    version = "0.1.0",
    about = "OpenPTZ: Cross-Platform PTZ Camera Controller with Live Video, Web GUI, TUI, and REST API"
)]
pub struct Cli {
    /// Device path (e.g. /dev/video2). If omitted, automatically selects the best camera.
    #[arg(short, long, global = true)]
    pub device: Option<PathBuf>,

    /// Subcommand to execute (if none specified, launches interactive TUI)
    #[command(subcommand)]
    pub command: Option<Commands>,

    // =========================================================================
    // Compatibility flags with xMRi/PTZControl (Windows)
    // =========================================================================
    #[arg(long = "move_up", hide = true)]
    pub compat_move_up: bool,

    #[arg(long = "move_down", hide = true)]
    pub compat_move_down: bool,

    #[arg(long = "move_left", hide = true)]
    pub compat_move_left: bool,

    #[arg(long = "move_right", hide = true)]
    pub compat_move_right: bool,

    #[arg(long = "zoom_in", hide = true)]
    pub compat_zoom_in: bool,

    #[arg(long = "zoom_out", hide = true)]
    pub compat_zoom_out: bool,

    #[arg(long = "move_home", hide = true)]
    pub compat_move_home: bool,

    #[arg(long = "store", hide = true)]
    pub compat_store: Option<u8>,

    #[arg(long = "restore", hide = true)]
    pub compat_restore: Option<u8>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Launch the graphical UI application (Live Video + D-Pad + Presets)
    Ui {
        /// HTTP port to bind to
        #[arg(short, long, default_value_t = 8080)]
        port: u16,

        /// Do not automatically open a browser window
        #[arg(long)]
        no_browser: bool,
    },

    /// Alias for ui (Graphical User Interface)
    Gui {
        /// HTTP port to bind to
        #[arg(short, long, default_value_t = 8080)]
        port: u16,

        /// Do not automatically open a browser window
        #[arg(long)]
        no_browser: bool,
    },

    /// Launch the interactive Terminal User Interface (TUI)
    Tui,

    /// List all detected video devices on the system
    List,

    /// Show current camera status (angles, zoom, sensor settings)
    Status {
        /// Output status in JSON format
        #[arg(short, long)]
        json: bool,
    },

    /// Return camera to Home position
    Home,

    /// Pan camera horizontally (left / right / to <deg>)
    Pan(PanArgs),

    /// Tilt camera vertically (up / down / to <deg>)
    Tilt(TiltArgs),

    /// Zoom camera (in / out / to <factor>)
    Zoom(ZoomArgs),

    /// Move Pan and Tilt simultaneously
    Move {
        /// Absolute Pan in degrees (-90.0 to +90.0)
        #[arg(long)]
        pan: Option<f32>,

        /// Absolute Tilt in degrees (-90.0 to +50.0)
        #[arg(long)]
        tilt: Option<f32>,

        /// Relative Pan delta in degrees
        #[arg(long)]
        pan_delta: Option<f32>,

        /// Relative Tilt delta in degrees
        #[arg(long)]
        tilt_delta: Option<f32>,
    },

    /// Manage hardware (1-8) and software presets
    Preset(PresetArgs),

    /// Set camera and image settings (anti-flicker frequency, brightness, exposure)
    Set(SetArgs),

    /// Start lightweight HTTP REST server for Stream Deck / Bitfocus Companion
    Serve {
        /// HTTP port to bind to
        #[arg(short, long, default_value_t = 8080)]
        port: u16,
    },
}

#[derive(Args, Debug)]
pub struct PanArgs {
    /// Move left by step degrees
    #[arg(long)]
    pub left: bool,

    /// Move right by step degrees
    #[arg(long)]
    pub right: bool,

    /// Move to absolute angle in degrees (-90.0 to +90.0)
    #[arg(long)]
    pub to: Option<f32>,

    /// Step in degrees for relative moves (default 5.0°)
    #[arg(short, long, default_value_t = 5.0)]
    pub step: f32,
}

#[derive(Args, Debug)]
pub struct TiltArgs {
    /// Move up by step degrees
    #[arg(long)]
    pub up: bool,

    /// Move down by step degrees
    #[arg(long)]
    pub down: bool,

    /// Move to absolute angle in degrees (-90.0 to +50.0)
    #[arg(long)]
    pub to: Option<f32>,

    /// Step in degrees for relative moves (default 5.0°)
    #[arg(short, long, default_value_t = 5.0)]
    pub step: f32,
}

#[derive(Args, Debug)]
pub struct ZoomArgs {
    /// Zoom in by step factor
    #[arg(long)]
    pub zoom_in: bool,

    /// Zoom out by step factor
    #[arg(long)]
    pub zoom_out: bool,

    /// Set absolute zoom multiplier (1.0x to 15.0x)
    #[arg(long)]
    pub to: Option<f32>,

    /// Step multiplier for relative zoom (default 0.5x)
    #[arg(short, long, default_value_t = 0.5)]
    pub step: f32,
}

#[derive(Args, Debug)]
pub struct PresetArgs {
    /// Recall a hardware preset (1 to 8)
    #[arg(long)]
    pub recall: Option<u8>,

    /// Save current position to a hardware preset (1 to 8)
    #[arg(long)]
    pub save: Option<u8>,

    /// Save a named software preset
    #[arg(long)]
    pub save_soft: Option<String>,

    /// Recall a named software preset
    #[arg(long)]
    pub recall_soft: Option<String>,

    /// List all saved software presets
    #[arg(long)]
    pub list_soft: bool,

    /// Delete a software preset
    #[arg(long)]
    pub delete_soft: Option<String>,
}

#[derive(Args, Debug)]
pub struct SetArgs {
    /// Anti-flicker power line frequency in Hz: 0 (Disabled), 50 (50Hz), 60 (60Hz)
    #[arg(long)]
    pub flicker: Option<u32>,

    /// Brightness (0 to 255)
    #[arg(long)]
    pub brightness: Option<i32>,

    /// Contrast (0 to 255)
    #[arg(long)]
    pub contrast: Option<i32>,

    /// Saturation (0 to 255)
    #[arg(long)]
    pub saturation: Option<i32>,

    /// Sharpness (0 to 255)
    #[arg(long)]
    pub sharpness: Option<i32>,

    /// Auto exposure enabled (true/false)
    #[arg(long)]
    pub auto_exposure: Option<bool>,

    /// Exposure time absolute
    #[arg(long)]
    pub exposure_time: Option<i32>,

    /// Auto focus enabled (true/false)
    #[arg(long)]
    pub auto_focus: Option<bool>,
}
