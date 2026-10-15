use std::sync::Arc;

use super::generic::GenericUvcProfile;
use super::logitech_rally::LogitechRallyProfile;
use super::traits::CameraProfile;

/// Registry of known camera profiles.
pub struct CameraRegistry {
    profiles: Vec<Arc<dyn CameraProfile>>,
}

impl CameraRegistry {
    /// Create registry initialized with default built-in profiles.
    pub fn new() -> Self {
        Self {
            profiles: vec![
                Arc::new(LogitechRallyProfile::new()),
                Arc::new(GenericUvcProfile::new()),
            ],
        }
    }

    /// Register a new camera profile before the fallback generic profile.
    #[allow(dead_code)]
    pub fn register(&mut self, profile: Arc<dyn CameraProfile>) {
        let insert_idx = self.profiles.len().saturating_sub(1);
        self.profiles.insert(insert_idx, profile);
    }

    /// List all registered profiles.
    #[allow(dead_code)]
    pub fn profiles(&self) -> &[Arc<dyn CameraProfile>] {
        &self.profiles
    }

    /// Find matching profile or fallback to GenericUvcProfile.
    pub fn find(&self, card_name: &str, usb_vid_pid: Option<(u16, u16)>) -> Arc<dyn CameraProfile> {
        for profile in &self.profiles {
            if profile.matches(card_name, usb_vid_pid) {
                return Arc::clone(profile);
            }
        }
        Arc::new(GenericUvcProfile::new())
    }

    /// Convenience detection using default registry.
    pub fn detect(card_name: &str, usb_vid_pid: Option<(u16, u16)>) -> Arc<dyn CameraProfile> {
        let registry = Self::new();
        registry.find(card_name, usb_vid_pid)
    }
}

impl Default for CameraRegistry {
    fn default() -> Self {
        Self::new()
    }
}
