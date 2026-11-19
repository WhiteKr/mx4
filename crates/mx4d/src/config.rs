//! The daemon's config file: `[device]` plus one section per module.
//!
//! Keys missing from the file keep their defaults, key by key, so a config
//! that sets one waveform does not reset the rest of its section.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use toml::{Table, Value};
use tracing::{info, warn};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub device: DeviceSection,
    #[cfg(feature = "haptic")]
    pub haptic: mx4_haptic::Config,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceSection {
    /// A /dev/hidraw* path, or "auto".
    pub path: String,
    /// 0xFF for Bluetooth direct, 1..6 for a receiver slot.
    pub number: u8,
    /// HAPTIC feature index. Asked from the mouse when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature_index: Option<u8>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            device: DeviceSection {
                path: "auto".into(),
                number: 0xFF,
                feature_index: None,
            },
            #[cfg(feature = "haptic")]
            haptic: mx4_haptic::Config::default(),
        }
    }
}

const MODULES: &[&str] = &["haptic"];

pub fn default_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("mx4").join("config.toml")
}

pub fn load(path: &Path) -> Result<Config> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text).with_context(|| format!("config {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            info!("No config file at {}, using defaults", path.display());
            Ok(Config::default())
        }
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

pub fn parse(text: &str) -> Result<Config> {
    let mut user: Table = text.parse()?;
    let mut merged = Table::try_from(Config::default())?;
    for key in user.keys().cloned().collect::<Vec<_>>() {
        if !merged.contains_key(&key) {
            if MODULES.contains(&key.as_str()) {
                warn!("[{key}] ignored: this build does not include that module");
            } else {
                anyhow::bail!("unknown section [{key}]");
            }
            user.remove(&key);
        }
    }
    merge(&mut merged, user);
    Ok(Value::Table(merged).try_into()?)
}

fn merge(base: &mut Table, over: Table) {
    for (k, v) in over {
        match (base.get_mut(&k), v) {
            (Some(Value::Table(b)), Value::Table(o)) => merge(b, o),
            (_, v) => {
                base.insert(k, v);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_config_matches_defaults() {
        let shipped = include_str!("../../../config.toml");
        assert_eq!(parse(shipped).unwrap(), Config::default());
    }

    #[test]
    fn user_values_merge_key_by_key() {
        let cfg = parse("[device]\nnumber = 2\n[haptic.notifications]\ncooldown_ms = 9\n").unwrap();
        assert_eq!(cfg.device.number, 2);
        assert_eq!(cfg.device.path, "auto");
        assert_eq!(cfg.haptic.notifications.cooldown_ms, 9);
        assert_eq!(cfg.haptic.notifications.waveform, "HAPPY_ALERT");
        assert!(cfg.haptic.focus_change.enabled);
    }

    #[test]
    fn typos_are_errors() {
        assert!(parse("[haptic.focus_change]\nwavefrom = \"KNOCK\"\n").is_err());
        assert!(parse("[devcie]\npath = \"auto\"\n").is_err());
    }

    #[test]
    fn hex_device_number_and_fixed_feature_index() {
        let cfg = parse("[device]\nnumber = 0xFF\nfeature_index = 11\n").unwrap();
        assert_eq!(cfg.device.number, 0xFF);
        assert_eq!(cfg.device.feature_index, Some(11));
    }
}
