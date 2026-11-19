//! An open connection to the MX Master 4's hidraw node.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use tracing::{debug, info, warn};

use crate::hidraw::{self, HIDPP_LONG_REPORT_ID, HIDPP_SHORT_REPORT_ID, TARGET_NAME};

pub const HIDPP_LONG_MSG_SIZE: usize = 20;
/// Feature index of a HID++ 2.0 error response.
const HIDPP_ERROR: u8 = 0xFF;
/// Tags our queries so their replies can be told apart from the kernel driver's.
const REQUEST_SW_ID: u8 = 0x0A;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(1);

pub const FEATURE_ROOT: u8 = 0x00;

/// Where to look for the mouse, and how to address it.
#[derive(Debug, Clone)]
pub struct DeviceConfig {
    /// A /dev/hidraw* path, or "auto" to find the mouse by its sysfs identity.
    pub path: String,
    /// 0xFF for Bluetooth direct, 1..6 for a receiver slot.
    pub number: u8,
    pub sysfs_dir: PathBuf,
    pub dev_dir: PathBuf,
}

impl DeviceConfig {
    pub fn new(path: impl Into<String>, number: u8) -> Self {
        Self {
            path: path.into(),
            number,
            sysfs_dir: "/sys/class/hidraw".into(),
            dev_dir: "/dev".into(),
        }
    }
}

/// The mouse's hidraw node, opened on demand and reopened after a failure.
///
/// Packets only go to a node that sysfs identifies as the MX Master 4 (or,
/// when `number` addresses a receiver slot, a Logitech receiver). hidraw
/// numbers move between boots and reconnects, so the configured path is
/// checked on every open, and a scan never falls back to "whatever opens".
#[derive(Debug)]
pub struct Device {
    cfg: DeviceConfig,
    file: Option<File>,
    path: Option<PathBuf>,
    /// The open node is a real device that can answer requests, not a stand-in file.
    answers: bool,
}

impl Device {
    pub fn new(cfg: DeviceConfig) -> Self {
        Self {
            cfg,
            file: None,
            path: None,
            answers: false,
        }
    }

    pub fn number(&self) -> u8 {
        self.cfg.number
    }

    /// The node currently open, or the configured path if none is.
    pub fn path(&self) -> String {
        match &self.path {
            Some(p) => p.display().to_string(),
            None => self.cfg.path.clone(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.file.is_some()
    }

    /// True if the open node can answer HID++ requests (a real hidraw device).
    pub fn answers_requests(&self) -> bool {
        self.file.is_some() && self.answers
    }

    pub fn close(&mut self) {
        self.file = None;
        self.path = None;
    }

    /// Open the configured path if it is the mouse, else the first node a scan finds.
    pub fn open(&mut self) -> io::Result<()> {
        self.close();
        let configured = PathBuf::from(&self.cfg.path);
        if self.cfg.path != "auto" {
            match self.check(&configured) {
                Ok((answers, why)) => match self.open_path(&configured, answers) {
                    Ok(()) => {
                        debug!("{}: {why}", configured.display());
                        return Ok(());
                    }
                    Err(e) => warn!("Cannot open {}: {e}", configured.display()),
                },
                Err(why) => warn!("Not using {}: {why}", configured.display()),
            }
        }
        for path in hidraw::scan(&self.cfg.sysfs_dir, &self.cfg.dev_dir, self.cfg.number) {
            if path == configured {
                continue;
            }
            match self.open_path(&path, true) {
                Ok(()) => {
                    info!("Found {TARGET_NAME} at {}", path.display());
                    return Ok(());
                }
                Err(e) => warn!("Cannot open {}: {e}", path.display()),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{TARGET_NAME} not found on any hidraw device"),
        ))
    }

    /// Whether `path` may be written to, and whether it can answer requests.
    fn check(&self, path: &Path) -> Result<(bool, String), String> {
        let meta = fs::metadata(path).map_err(|e| e.to_string())?;
        if meta.is_file() {
            // A regular file is not a HID device and cannot reach one; tests
            // and the verify skill use it as a stand-in mouse.
            return Ok((false, "regular file, not a device node".into()));
        }
        if !meta.file_type().is_char_device() {
            return Err("not a device node".into());
        }
        let real = fs::canonicalize(path).map_err(|e| e.to_string())?;
        let name = real
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let info = hidraw::hidraw_info(&self.cfg.sysfs_dir, name)
            .ok_or_else(|| format!("no sysfs entry for {name}"))?;
        let desc = format!(
            "{:?} ({:04x}:{:04x})",
            info.hid_name, info.vendor, info.product
        );
        if hidraw::is_target(&info, self.cfg.number) {
            Ok((true, desc))
        } else {
            Err(format!("{name} is {desc}, not {TARGET_NAME}"))
        }
    }

    fn open_path(&mut self, path: &Path, answers: bool) -> io::Result<()> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        info!("Opened {}", path.display());
        self.file = Some(file);
        self.path = Some(path.to_owned());
        self.answers = answers;
        Ok(())
    }

    fn file(&mut self) -> io::Result<&mut File> {
        if self.file.is_none() {
            self.open()?;
        }
        Ok(self.file.as_mut().expect("opened above"))
    }

    /// Write one 20-byte HID++ long report, opening the device if needed.
    /// The device is closed on failure so the next call reconnects.
    pub fn write(&mut self, packet: &[u8; HIDPP_LONG_MSG_SIZE]) -> io::Result<()> {
        let result = self.file().and_then(|f| f.write_all(packet));
        if result.is_err() {
            self.close();
        }
        result
    }

    /// Build a HID++ long report addressed to this device.
    pub fn packet(
        &self,
        feature_index: u8,
        fn_byte: u8,
        params: &[u8],
    ) -> [u8; HIDPP_LONG_MSG_SIZE] {
        let mut p = [0u8; HIDPP_LONG_MSG_SIZE];
        p[..4].copy_from_slice(&[
            HIDPP_LONG_REPORT_ID,
            self.cfg.number,
            feature_index,
            fn_byte,
        ]);
        let n = params.len().min(HIDPP_LONG_MSG_SIZE - 4);
        p[4..4 + n].copy_from_slice(&params[..n]);
        p
    }

    /// Send a HID++ 2.0 query and return the reply's parameter bytes.
    pub fn request(
        &mut self,
        feature_index: u8,
        function: u8,
        params: &[u8],
    ) -> io::Result<Vec<u8>> {
        if self.file.is_none() {
            self.open()?;
        }
        if !self.answers {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "device cannot answer requests",
            ));
        }
        let fn_byte = (function << 4) | REQUEST_SW_ID;
        let packet = self.packet(feature_index, fn_byte, params);
        let result = self.exchange(&packet, feature_index, fn_byte);
        if result.is_err() {
            self.close();
        }
        result
    }

    fn exchange(
        &mut self,
        packet: &[u8; HIDPP_LONG_MSG_SIZE],
        feature_index: u8,
        fn_byte: u8,
    ) -> io::Result<Vec<u8>> {
        let file = self.file()?;
        file.write_all(packet)?;
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        let mut buf = [0u8; 64];
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            let timeout = Timespec::try_from(left).map_err(io::Error::other)?;
            let mut fds = [PollFd::new(&*file, PollFlags::IN)];
            if poll(&mut fds, Some(&timeout))? == 0 {
                break;
            }
            let n = file.read(&mut buf)?;
            let r = &buf[..n];
            // Mouse movement and the kernel driver's traffic share this node; skip them.
            if n < 7 || !matches!(r[0], HIDPP_SHORT_REPORT_ID | HIDPP_LONG_REPORT_ID) {
                continue;
            }
            if r[2] == feature_index && r[3] == fn_byte {
                return Ok(r[4..].to_vec());
            }
            if r[2] == HIDPP_ERROR && r[3] == feature_index && r[4] == fn_byte {
                return Err(io::Error::other(format!(
                    "HID++ error 0x{:02X} for feature index {feature_index}",
                    r[5]
                )));
            }
        }
        Err(io::Error::new(io::ErrorKind::TimedOut, "no HID++ reply"))
    }

    /// Look up a feature's index on the device through the Root feature.
    pub fn feature_index(&mut self, feature_id: u16) -> io::Result<u8> {
        let reply = self.request(FEATURE_ROOT, 0, &feature_id.to_be_bytes())?;
        match reply.first() {
            Some(&index) if index != 0 => Ok(index),
            _ => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("device lacks HID++ feature 0x{feature_id:04X}"),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regular_file_is_written_but_never_queried() {
        let f = tempfile::NamedTempFile::new().unwrap();
        let mut dev = Device::new(DeviceConfig::new(f.path().to_str().unwrap(), 0xFF));
        let pkt = dev.packet(11, 0x41, &[5]);
        dev.write(&pkt).unwrap();
        assert!(!dev.answers_requests());
        assert_eq!(
            dev.request(0, 0, &[0x19, 0xB0]).unwrap_err().kind(),
            io::ErrorKind::Unsupported
        );
        let mut expected = vec![0x11, 0xFF, 11, 0x41, 5];
        expected.resize(20, 0);
        assert_eq!(fs::read(f.path()).unwrap(), expected);
    }

    #[test]
    fn missing_mouse_is_an_error_not_a_fallback() {
        let mut cfg = DeviceConfig::new("auto", 0xFF);
        cfg.sysfs_dir = "/nonexistent".into();
        let mut dev = Device::new(cfg);
        assert_eq!(
            dev.write(&[0; 20]).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert!(!dev.is_open());
    }

    #[test]
    fn directory_is_not_a_device() {
        let d = tempfile::tempdir().unwrap();
        let mut cfg = DeviceConfig::new(d.path().to_str().unwrap(), 0xFF);
        cfg.sysfs_dir = "/nonexistent".into();
        assert!(Device::new(cfg).open().is_err());
    }
}
