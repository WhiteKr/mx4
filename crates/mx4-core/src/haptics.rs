//! Playing haptic waveforms through the HAPTIC feature (0x19B0).

use std::io;
use std::sync::{Arc, Mutex};

use tracing::{debug, info, warn};

use crate::device::Device;

pub const FEATURE_HAPTIC: u16 = 0x19B0;
const PLAY_FN: u8 = 4;
const SOFTWARE_ID: u8 = 1;
/// The HAPTIC index on the MX Master 4, used when the node cannot be asked.
pub const DEFAULT_FEATURE_INDEX: u8 = 11;

pub const WAVEFORMS: &[(&str, u8)] = &[
    ("SHARP_STATE_CHANGE", 0x00),
    ("DAMP_STATE_CHANGE", 0x01),
    ("SHARP_COLLISION", 0x02),
    ("DAMP_COLLISION", 0x03),
    ("SUBTLE_COLLISION", 0x04),
    ("HAPPY_ALERT", 0x05),
    ("ANGRY_ALERT", 0x06),
    ("COMPLETED", 0x07),
    ("SQUARE", 0x08),
    ("WAVE", 0x09),
    ("FIREWORK", 0x0A),
    ("MAD", 0x0B),
    ("KNOCK", 0x0C),
    ("JINGLE", 0x0D),
    ("RINGING", 0x0E),
    ("WHISPER_COLLISION", 0x1B),
];

pub fn waveform(name: &str) -> Option<u8> {
    WAVEFORMS.iter().find(|(n, _)| *n == name).map(|&(_, i)| i)
}

/// Plays waveforms on a shared [`Device`]. Cheap to clone; every module gets one.
#[derive(Clone)]
pub struct Haptics {
    inner: Arc<Mutex<State>>,
}

struct State {
    device: Device,
    /// Configured HAPTIC index; when None it is asked from the mouse after each open.
    fixed_index: Option<u8>,
    index: Option<u8>,
}

impl Haptics {
    pub fn new(device: Device, feature_index: Option<u8>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(State {
                device,
                fixed_index: feature_index,
                index: None,
            })),
        }
    }

    /// The node in use, for logs.
    pub fn device_path(&self) -> String {
        self.lock().device.path()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Open the mouse now so problems show at startup. Errors are only logged.
    pub fn connect(&self) {
        let mut s = self.lock();
        match s.device.open().and_then(|()| s.ensure_index()) {
            Ok(index) => info!("HAPTIC feature index {index} on {}", s.device.path()),
            Err(e) => warn!("Mouse not ready yet: {e}"),
        }
    }

    /// Play a waveform by name. Returns false if it could not be sent.
    pub fn play(&self, name: &str) -> bool {
        match waveform(name) {
            Some(i) => self.play_index(i),
            None => {
                warn!("Unknown waveform: {name}");
                false
            }
        }
    }

    pub fn play_index(&self, waveform: u8) -> bool {
        let mut s = self.lock();
        for attempt in 0..2 {
            match s.send(waveform) {
                Ok(()) => {
                    debug!("Sent waveform 0x{waveform:02X}");
                    return true;
                }
                Err(e) if attempt == 0 => debug!("Write failed ({e}), reconnecting"),
                Err(e) => warn!("Haptic not sent: {e}"),
            }
        }
        false
    }
}

impl State {
    fn send(&mut self, waveform: u8) -> io::Result<()> {
        if !self.device.is_open() {
            self.index = None;
            self.device.open()?;
        }
        let index = self.ensure_index()?;
        let packet = self
            .device
            .packet(index, PLAY_FN << 4 | SOFTWARE_ID, &[waveform]);
        self.device.write(&packet)
    }

    fn ensure_index(&mut self) -> io::Result<u8> {
        if let Some(i) = self.fixed_index.or(self.index) {
            return Ok(i);
        }
        let index = if self.device.answers_requests() {
            self.device.feature_index(FEATURE_HAPTIC)?
        } else {
            DEFAULT_FEATURE_INDEX
        };
        self.index = Some(index);
        Ok(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::DeviceConfig;

    fn on_file(index: Option<u8>) -> (tempfile::NamedTempFile, Haptics) {
        let f = tempfile::NamedTempFile::new().unwrap();
        let dev = Device::new(DeviceConfig::new(f.path().to_str().unwrap(), 0xFF));
        (f, Haptics::new(dev, index))
    }

    #[test]
    fn play_writes_hidpp_long_report() {
        let (f, h) = on_file(None);
        assert!(h.play("HAPPY_ALERT"));
        let mut expected = vec![0x11, 0xFF, 11, 0x41, 0x05];
        expected.resize(20, 0);
        assert_eq!(std::fs::read(f.path()).unwrap(), expected);
    }

    #[test]
    fn configured_feature_index_wins() {
        let (f, h) = on_file(Some(7));
        assert!(h.play("KNOCK"));
        assert_eq!(
            &std::fs::read(f.path()).unwrap()[..5],
            &[0x11, 0xFF, 7, 0x41, 0x0C]
        );
    }

    #[test]
    fn unknown_waveform_writes_nothing() {
        let (f, h) = on_file(None);
        assert!(!h.play("NOPE"));
        assert!(std::fs::read(f.path()).unwrap().is_empty());
    }
}
