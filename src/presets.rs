use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::v4l2::CameraDevice;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub pan_deg: f32,
    pub tilt_deg: f32,
    pub zoom_x: f32,
    pub brightness: Option<i32>,
    pub power_line_frequency: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PresetManager {
    #[serde(default)]
    pub presets: BTreeMap<String, Preset>,
    #[serde(skip)]
    config_path: PathBuf,
}

impl PresetManager {
    pub fn config_path() -> PathBuf {
        if let Some(config_dir) = dirs::config_dir() {
            let primary = config_dir.join("openptz").join("presets.json");
            if primary.exists() {
                return primary;
            }
            let legacy = config_dir.join("ptz-rally").join("presets.json");
            if legacy.exists() {
                return legacy;
            }
            primary
        } else {
            PathBuf::from("presets.json")
        }
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(file) = File::open(&path) {
                let reader = BufReader::new(file);
                if let Ok(mut mgr) = serde_json::from_reader::<_, PresetManager>(reader) {
                    mgr.config_path = path;
                    return mgr;
                }
            }
        }

        Self {
            presets: BTreeMap::new(),
            config_path: path,
        }
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory {:?}", parent))?;
        }

        let file = File::create(&self.config_path)
            .with_context(|| format!("Failed to create preset file {:?}", self.config_path))?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, self)?;
        Ok(())
    }

    pub fn set_preset(
        &mut self,
        name: &str,
        pan_deg: f32,
        tilt_deg: f32,
        zoom_x: f32,
        brightness: Option<i32>,
        power_line_frequency: Option<i32>,
    ) -> Result<Preset> {
        let preset = Preset {
            name: name.to_string(),
            pan_deg,
            tilt_deg,
            zoom_x,
            brightness,
            power_line_frequency,
        };
        self.presets.insert(name.to_string(), preset.clone());
        self.save()?;
        Ok(preset)
    }

    /// Save current camera coordinates into a numbered slot (1..8)
    pub fn save_slot(&mut self, slot: u8, dev: &CameraDevice) -> Result<Preset> {
        let pan = dev.get_pan_deg()?;
        let tilt = dev.get_tilt_deg()?;
        let zoom = dev.get_zoom_x()?;
        let brightness = dev.get_brightness().ok();
        let freq = dev.get_power_line_frequency().ok();

        self.set_preset(&slot.to_string(), pan, tilt, zoom, brightness, freq)
    }

    /// Recall a numbered slot (1..8) on camera
    pub fn recall_slot(&self, slot: u8, dev: &CameraDevice) -> Result<Preset> {
        self.apply(dev, &slot.to_string())
    }

    #[allow(dead_code)]
    pub fn get_slot(&self, slot: u8) -> Option<&Preset> {
        self.presets.get(&slot.to_string())
    }

    pub fn delete_preset(&mut self, name: &str) -> Result<bool> {
        let removed = self.presets.remove(name).is_some();
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    #[allow(dead_code)]
    pub fn get_preset(&self, name: &str) -> Option<&Preset> {
        self.presets.get(name)
    }

    pub fn list(&self) -> Vec<&Preset> {
        self.presets.values().collect()
    }

    pub fn apply(&self, dev: &CameraDevice, name: &str) -> Result<Preset> {
        let preset = match self.presets.get(name) {
            Some(p) => p.clone(),
            None => bail!("Preset '{}' is not saved yet", name),
        };

        dev.move_pan_tilt(preset.pan_deg, preset.tilt_deg)?;
        dev.set_zoom_x(preset.zoom_x)?;

        if let Some(b) = preset.brightness {
            let _ = dev.set_brightness(b);
        }
        if let Some(f) = preset.power_line_frequency {
            let _ = dev.set_power_line_frequency(f as u32);
        }

        Ok(preset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preset_serialization() {
        let mut mgr = PresetManager::default();
        mgr.presets.insert(
            "1".to_string(),
            Preset {
                name: "1".to_string(),
                pan_deg: 15.0,
                tilt_deg: -5.0,
                zoom_x: 2.0,
                brightness: Some(128),
                power_line_frequency: Some(1),
            },
        );

        let json = serde_json::to_string_pretty(&mgr).unwrap();
        assert!(json.contains("\"pan_deg\": 15.0"));
        assert!(json.contains("\"name\": \"1\""));

        let deserialized: PresetManager = serde_json::from_str(&json).unwrap();
        let p1 = deserialized.presets.get("1").unwrap();
        assert_eq!(p1.pan_deg, 15.0);
        assert_eq!(p1.tilt_deg, -5.0);
        assert_eq!(p1.zoom_x, 2.0);
    }
}
