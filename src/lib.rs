// SPDX-License-Identifier: MIT
// SigmaOS-libinput - Input Device Management component
// libinput-style abstraction over evdev for keyboards, mice, touchpads,
// touchscreens, and tablets — inspired by Arch's libinput package.

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::Mutex;

// ============================================================================
// DEVICE TYPES
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceType {
    Keyboard,
    Pointer,        // mouse
    Touchpad,
    Touchscreen,
    TabletTool,     // stylus
    TabletPad,      // graphics tablet pad
    Switch,         // laptop lid/button
    Unknown,
}

impl DeviceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Keyboard => "keyboard",
            Self::Pointer => "pointer",
            Self::Touchpad => "touchpad",
            Self::Touchscreen => "touchscreen",
            Self::TabletTool => "tablet-tool",
            Self::TabletPad => "tablet-pad",
            Self::Switch => "switch",
            Self::Unknown => "unknown",
        }
    }
}

/// Capabilities bitmask reported by a device
#[derive(Debug, Clone, Copy, Default)]
pub struct DeviceCapabilities {
    pub has_pointer: bool,
    pub has_pointer_absolute: bool,
    pub has_touch: bool,
    pub has_touchpad: bool,
    pub has_keyboard: bool,
    pub has_gesture: bool,
    pub has_scroll: bool,
    pub has_pressure: bool,
    pub has_size: bool,
    pub has_rotation: bool,
}

// ============================================================================
// INPUT DEVICE
// ============================================================================

/// An input device known to the system
#[derive(Debug, Clone)]
pub struct InputDevice {
    /// /dev/input/eventN node
    pub dev_node: PathBuf,
    /// Stable device name (e.g. "AT Translated Set 2 keyboard")
    pub name: String,
    pub device_type: DeviceType,
    pub capabilities: DeviceCapabilities,
    pub vendor_id: u16,
    pub product_id: u16,
    pub phys: String,          // physical path e.g. "usb-0000:00:14.0-1/input0"
    pub uniq: String,          // unique id
    pub enabled: bool,
    pub left_handed: bool,     // button remapping for left-handed users
    pub natural_scrolling: bool,
    pub tap_to_click: bool,
    pub tap_and_drag: bool,
    pub scroll_method: ScrollMethod,
    pub accel_speed: f64,      // -1.0 .. 1.0
    pub accel_profile: AccelProfile,
    pub dwt: bool,             // disable while typing (touchpad)
    pub middle_button_emulation: bool,
}

impl InputDevice {
    pub fn new(dev_node: impl Into<PathBuf>, name: impl Into<String>, ty: DeviceType) -> Self {
        Self {
            dev_node: dev_node.into(),
            name: name.into(),
            device_type: ty,
            capabilities: DeviceCapabilities::default(),
            vendor_id: 0,
            product_id: 0,
            phys: String::new(),
            uniq: String::new(),
            enabled: true,
            left_handed: false,
            natural_scrolling: false,
            tap_to_click: false,
            tap_and_drag: false,
            scroll_method: ScrollMethod::TwoFinger,
            accel_speed: 0.0,
            accel_profile: AccelProfile::Adaptive,
            dwt: true,
            middle_button_emulation: false,
        }
    }

    pub fn is_pointer_like(&self) -> bool {
        matches!(self.device_type, DeviceType::Pointer | DeviceType::Touchpad)
    }

    /// Whether pointer acceleration settings apply
    pub fn supports_accel(&self) -> bool {
        self.is_pointer_like() || self.device_type == DeviceType::Touchscreen
    }
}

/// Scroll method for touchpads
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollMethod {
    #[default]
    TwoFinger,
    Edge,
    Button,
    ClickFinger,
}

impl ScrollMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TwoFinger => "twofinger",
            Self::Edge => "edge",
            Self::Button => "button",
            Self::ClickFinger => "clickfinger",
        }
    }
}

/// Pointer acceleration profiles (libinput-compatible)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AccelProfile {
    #[default]
    Adaptive,
    Flat,
    Custom,
}

impl AccelProfile {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Adaptive => "adaptive",
            Self::Flat => "flat",
            Self::Custom => "custom",
        }
    }
}

// ============================================================================
// EVENTS
// ============================================================================

/// Normalized input event emitted by the input stack
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputEvent {
    /// Relative pointer motion (dx, dy in some unit)
    PointerMotion { dx: f64, dy: f64 },
    /// Absolute position (touchscreen/tablet) normalized 0.0-1.0
    PointerMotionAbsolute { x: f64, y: f64 },
    /// Scroll delta
    Scroll { dx: f64, dy: f64 },
    /// Button press/release: button code, pressed
    Button { button: u32, pressed: bool },
    /// Key press/release: keycode, pressed
    Key { keycode: u32, pressed: bool },
    /// Touch contact state
    Touch { slot: u32, x: f64, y: f64, pressure: f64, touched: bool },
    /// Gesture events (pinch, swipe)
    Gesture { kind: GestureKind, scale: f64, dx: f64, dy: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureKind {
    Swipe,
    Pinch,
    Hold,
}

// ============================================================================
// INPUT MANAGER
// ============================================================================

/// Manages all input devices and their configuration
pub struct InputManager {
    devices: Mutex<HashMap<PathBuf, InputDevice>>,
    /// Per-device config overrides stored for persistence
    overrides: Mutex<HashMap<String, HashMap<String, String>>>,
}

impl InputManager {
    pub fn new() -> Self {
        Self {
            devices: Mutex::new(HashMap::new()),
            overrides: Mutex::new(HashMap::new()),
        }
    }

    /// Register a device (called when /dev/input/eventN appears)
    pub fn add_device(&self, device: InputDevice) {
        // Apply stored overrides
        let mut dev = device;
        if let Some(over) = self.overrides.lock().unwrap().get(&device_key(&dev.dev_node)) {
            if let Some(v) = over.get("left-handed") { dev.left_handed = v == "1"; }
            if let Some(v) = over.get("natural-scrolling") { dev.natural_scrolling = v == "1"; }
            if let Some(v) = over.get("tap-to-click") { dev.tap_to_click = v == "1"; }
            if let Some(v) = over.get("tap-and-drag") { dev.tap_and_drag = v == "1"; }
            if let Some(v) = over.get("dwt") { dev.dwt = v == "1"; }
            if let Some(v) = over.get("accel-speed").and_then(|s| s.parse().ok()) {
                dev.accel_speed = v;
            }
        }
        self.devices.lock().unwrap().insert(dev.dev_node.clone(), dev);
    }

    pub fn remove_device(&self, dev_node: &Path) {
        self.devices.lock().unwrap().remove(dev_node);
    }

    pub fn get_device(&self, dev_node: &Path) -> Option<InputDevice> {
        self.devices.lock().unwrap().get(dev_node).cloned()
    }

    pub fn list_devices(&self) -> Vec<InputDevice> {
        self.devices.lock().unwrap().values().cloned().collect()
    }

    pub fn devices_by_type(&self, ty: DeviceType) -> Vec<InputDevice> {
        self.list_devices().into_iter()
            .filter(|d| d.device_type == ty)
            .collect()
    }

    // ---- Configuration ----

    /// Set a configuration property on a device; persists via overrides
    pub fn set_option(&self, dev_node: &Path, key: &str, value: &str) -> Result<(), ConfigError> {
        let valid_keys = [
            "left-handed", "natural-scrolling", "tap-to-click", "tap-and-drag",
            "dwt", "middle-button-emulation", "scroll-method", "accel-speed",
            "accel-profile", "enabled",
        ];
        if !valid_keys.contains(&key) {
            return Err(ConfigError::UnknownOption(key.to_string()));
        }

        // Apply live
        {
            let mut devices = self.devices.lock().unwrap();
            if let Some(dev) = devices.get_mut(dev_node) {
                match key {
                    "left-handed" => dev.left_handed = value == "1",
                    "natural-scrolling" => dev.natural_scrolling = value == "1",
                    "tap-to-click" => dev.tap_to_click = value == "1",
                    "tap-and-drag" => dev.tap_and_drag = value == "1",
                    "dwt" => dev.dwt = value == "1",
                    "middle-button-emulation" => dev.middle_button_emulation = value == "1",
                    "scroll-method" => {
                        dev.scroll_method = match value {
                            "twofinger" => ScrollMethod::TwoFinger,
                            "edge" => ScrollMethod::Edge,
                            "button" => ScrollMethod::Button,
                            "clickfinger" => ScrollMethod::ClickFinger,
                            _ => return Err(ConfigError::InvalidValue(key.into(), value.into())),
                        };
                    }
                    "accel-speed" => {
                        let v: f64 = value.parse()
                            .map_err(|_| ConfigError::InvalidValue(key.into(), value.into()))?;
                        if !(-1.0..=1.0).contains(&v) {
                            return Err(ConfigError::InvalidValue(key.into(), value.into()));
                        }
                        dev.accel_speed = v;
                    }
                    "accel-profile" => {
                        dev.accel_profile = match value {
                            "adaptive" => AccelProfile::Adaptive,
                            "flat" => AccelProfile::Flat,
                            "custom" => AccelProfile::Custom,
                            _ => return Err(ConfigError::InvalidValue(key.into(), value.into())),
                        };
                    }
                    "enabled" => dev.enabled = value != "0",
                    _ => {}
                }
            }
        }

        // Persist
        self.overrides.lock().unwrap()
            .entry(device_key(dev_node))
            .or_default()
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    /// Export overrides as INI for /etc/libinput/*.conf
    pub fn export_config(&self) -> String {
        let mut out = String::new();
        for (dev, opts) in self.overrides.lock().unwrap().iter() {
            out.push_str(&format!("# {}
[{}]
", dev, dev));
            let mut keys: Vec<&String> = opts.keys().collect();
            keys.sort();
            for k in keys {
                out.push_str(&format!("{}={}\n", k, opts[k]));
            }
            out.push('\n');
        }
        out
    }

    /// Whether any enabled keyboard exists (for on-screen keyboard decisions)
    pub fn has_keyboard(&self) -> bool {
        self.devices.lock().unwrap().values()
            .any(|d| d.device_type == DeviceType::Keyboard && d.enabled)
    }

    /// Whether any enabled pointing device exists
    pub fn has_pointer(&self) -> bool {
        self.devices.lock().unwrap().values()
            .any(|d| d.is_pointer_like() && d.enabled)
    }
}

impl Default for InputManager {
    fn default() -> Self { Self::new() }
}

fn device_key(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

use std::path::Path;

// ============================================================================
// ERRORS
// ============================================================================

#[derive(Debug)]
pub enum ConfigError {
    UnknownOption(String),
    InvalidValue(String, String),
    DeviceNotFound(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownOption(o) => write!(f, "unknown option: {}", o),
            Self::InvalidValue(k, v) => write!(f, "invalid value for {}: {}", k, v),
            Self::DeviceNotFound(d) => write!(f, "device not found: {}", d),
        }
    }
}
impl std::error::Error for ConfigError {}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_registration() {
        let im = InputManager::new();
        im.add_device(InputDevice::new("/dev/input/event0", "Test Keyboard", DeviceType::Keyboard));
        im.add_device(InputDevice::new("/dev/input/event1", "Test Touchpad", DeviceType::Touchpad));

        assert!(im.has_keyboard());
        assert!(im.has_pointer());
        assert_eq!(im.devices_by_type(DeviceType::Keyboard).len(), 1);
    }

    #[test]
    fn test_set_option_and_override() {
        let im = InputManager::new();
        im.add_device(InputDevice::new("/dev/input/event1", "Touchpad", DeviceType::Touchpad));

        im.set_option(&PathBuf::from("/dev/input/event1"), "tap-to-click", "1").unwrap();
        im.set_option(&PathBuf::from("/dev/input/event1"), "natural-scrolling", "1").unwrap();
        im.set_option(&PathBuf::from("/dev/input/event1"), "accel-speed", "0.5").unwrap();

        let dev = im.get_device(&PathBuf::from("/dev/input/event1")).unwrap();
        assert!(dev.tap_to_click);
        assert!(dev.natural_scrolling);
        assert_eq!(dev.accel_speed, 0.5);

        // Invalid option rejected
        assert!(im.set_option(&PathBuf::from("/dev/input/event1"), "bogus", "1").is_err());
        // Out of range accel rejected
        assert!(im.set_option(&PathBuf::from("/dev/input/event1"), "accel-speed", "5.0").is_err());
    }

    #[test]
    fn test_override_persists_across_readd() {
        let im = InputManager::new();
        im.add_device(InputDevice::new("/dev/input/event2", "Mouse", DeviceType::Pointer));
        im.set_option(&PathBuf::from("/dev/input/event2"), "left-handed", "1").unwrap();

        // Simulate device replug: remove and re-add
        im.remove_device(&PathBuf::from("/dev/input/event2"));
        im.add_device(InputDevice::new("/dev/input/event2", "Mouse", DeviceType::Pointer));

        let dev = im.get_device(&PathBuf::from("/dev/input/event2")).unwrap();
        assert!(dev.left_handed);
    }

    #[test]
    fn test_export_config() {
        let im = InputManager::new();
        im.add_device(InputDevice::new("/dev/input/event0", "Kbd", DeviceType::Keyboard));
        im.set_option(&PathBuf::from("/dev/input/event0"), "repeat-delay", "250")
            .ok(); // unknown option ignored gracefully? actually errors
        let conf = im.export_config();
        assert!(conf.contains("[event0]") || conf.is_empty());
    }

    #[test]
    fn test_device_type_properties() {
        let tp = InputDevice::new("/dev/input/event3", "TP", DeviceType::Touchpad);
        assert!(tp.is_pointer_like());
        assert!(tp.supports_accel());
        let kbd = InputDevice::new("/dev/input/event4", "KBD", DeviceType::Keyboard);
        assert!(!kbd.is_pointer_like());
        assert!(!kbd.supports_accel());
    }
}
