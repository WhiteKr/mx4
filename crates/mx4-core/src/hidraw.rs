//! Finding the MX Master 4 among /dev/hidraw* nodes through sysfs.

use std::fs;
use std::path::{Path, PathBuf};

pub const LOGITECH_VENDOR_ID: u32 = 0x046D;
pub const TARGET_NAME: &str = "MX Master 4";
pub const HIDPP_SHORT_REPORT_ID: u8 = 0x10;
pub const HIDPP_LONG_REPORT_ID: u8 = 0x11;
/// `device_number` that addresses a Bluetooth-direct device rather than a receiver slot.
pub const DIRECT_DEVICE: u8 = 0xFF;

/// What sysfs says about one hidraw node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HidrawInfo {
    pub vendor: u32,
    pub product: u32,
    pub hid_name: String,
    /// The report descriptor declares the HID++ long report.
    pub hidpp_long: bool,
}

/// Report IDs declared in a HID report descriptor.
pub fn report_ids(descriptor: &[u8]) -> Vec<u8> {
    let mut ids = Vec::new();
    let mut i = 0;
    while i < descriptor.len() {
        let prefix = descriptor[i];
        if prefix == 0xFE {
            // Long item: 0xFE, size, tag, data...
            let size = descriptor.get(i + 1).copied().unwrap_or(0) as usize;
            i += 3 + size;
            continue;
        }
        let size = [0, 1, 2, 4][(prefix & 0x03) as usize];
        if prefix & 0xFC == 0x84
            && size >= 1
            && let Some(&id) = descriptor.get(i + 1)
        {
            ids.push(id);
        }
        i += 1 + size;
    }
    ids
}

/// Read sysfs metadata for a hidraw node such as `hidraw4`.
pub fn hidraw_info(sysfs_dir: &Path, name: &str) -> Option<HidrawInfo> {
    let devdir = sysfs_dir.join(name).join("device");
    let uevent = fs::read_to_string(devdir.join("uevent")).ok()?;
    let descriptor = fs::read(devdir.join("report_descriptor")).ok()?;
    let field = |key: &str| {
        uevent
            .lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
            .map(str::to_owned)
    };
    // HID_ID=0005:0000046D:0000B042  (bus:vendor:product, hex)
    let id = field("HID_ID")?;
    let mut parts = id.split(':').map(|p| u32::from_str_radix(p, 16));
    let (_bus, vendor, product) = (
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    );
    Some(HidrawInfo {
        vendor,
        product,
        hid_name: field("HID_NAME").unwrap_or_default(),
        hidpp_long: report_ids(&descriptor).contains(&HIDPP_LONG_REPORT_ID),
    })
}

/// True if `info` is a node we may send HID++ packets to.
///
/// That is the MX Master 4 itself, or, when `device_number` addresses a
/// receiver slot (1..6), a Logitech receiver.
pub fn is_target(info: &HidrawInfo, device_number: u8) -> bool {
    if info.vendor != LOGITECH_VENDOR_ID || !info.hidpp_long {
        return false;
    }
    let name = info.hid_name.to_lowercase();
    name.contains(&TARGET_NAME.to_lowercase())
        || (device_number != DIRECT_DEVICE && name.contains("receiver"))
}

/// Every /dev path whose hidraw node passes [`is_target`], lowest number first.
pub fn scan(sysfs_dir: &Path, dev_dir: &Path, device_number: u8) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(sysfs_dir) else {
        return Vec::new();
    };
    let mut nodes: Vec<(u32, String)> = entries
        .filter_map(|e| {
            let name = e.ok()?.file_name().into_string().ok()?;
            let n = name.strip_prefix("hidraw")?.parse().ok()?;
            Some((n, name))
        })
        .collect();
    nodes.sort();
    nodes
        .into_iter()
        .filter(|(_, name)| {
            hidraw_info(sysfs_dir, name).is_some_and(|i| is_target(&i, device_number))
        })
        .map(|(_, name)| dev_dir.join(name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Vendor page 0xFF43 with Report IDs 0x10 (short) and 0x11 (long): a HID++ device.
    pub const HIDPP_DESC: &[u8] = &[
        0x06, 0x43, 0xFF, 0x0A, 0x02, 0x02, 0xA1, 0x01, 0x85, 0x10, 0x75, 0x08, 0x95, 0x06, 0x15,
        0x00, 0x26, 0xFF, 0x00, 0x09, 0x01, 0x81, 0x00, 0x09, 0x01, 0x91, 0x00, 0x85, 0x11, 0x95,
        0x13, 0x09, 0x02, 0x81, 0x00, 0x09, 0x02, 0x91, 0x00, 0xC0,
    ];
    // Plain keyboard: Report ID 0x01 only.
    const KEYBOARD_DESC: &[u8] = &[
        0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x85, 0x01, 0x05, 0x07, 0x19, 0xE0, 0x29, 0xE7, 0x15,
        0x00, 0x25, 0x01, 0x75, 0x01, 0x95, 0x08, 0x81, 0x02, 0xC0,
    ];

    fn node(root: &Path, name: &str, hid_id: &str, hid_name: &str, desc: &[u8]) {
        let d = root.join(name).join("device");
        fs::create_dir_all(&d).unwrap();
        fs::write(
            d.join("uevent"),
            format!("HID_ID={hid_id}\nHID_NAME={hid_name}\n"),
        )
        .unwrap();
        fs::write(d.join("report_descriptor"), desc).unwrap();
    }

    #[test]
    fn report_ids_from_descriptor() {
        assert_eq!(report_ids(HIDPP_DESC), vec![0x10, 0x11]);
        assert_eq!(report_ids(KEYBOARD_DESC), vec![0x01]);
        assert_eq!(
            report_ids(&[0xFE, 0x02, 0x00, 0x85, 0x11]),
            Vec::<u8>::new()
        );
    }

    #[test]
    fn scan_finds_only_the_mouse() {
        let root = tempfile::tempdir().unwrap();
        let r = root.path();
        node(
            r,
            "hidraw0",
            "0018:000014E5:0000E545",
            "ZNT0001:00 14E5:E545",
            KEYBOARD_DESC,
        );
        node(
            r,
            "hidraw1",
            "0003:0000046D:0000C548",
            "Logitech USB Receiver",
            HIDPP_DESC,
        );
        node(
            r,
            "hidraw10",
            "0005:0000046D:0000B042",
            "Logitech MX Master 4",
            HIDPP_DESC,
        );
        node(
            r,
            "hidraw2",
            "0005:0000046D:0000B042",
            "Logitech MX Master 4",
            KEYBOARD_DESC,
        );
        let dev = Path::new("/dev");
        assert_eq!(
            scan(r, dev, DIRECT_DEVICE),
            vec![PathBuf::from("/dev/hidraw10")]
        );
        assert_eq!(
            scan(r, dev, 1),
            vec![
                PathBuf::from("/dev/hidraw1"),
                PathBuf::from("/dev/hidraw10")
            ]
        );
    }

    #[test]
    fn other_vendor_with_same_name_is_rejected() {
        let info = HidrawInfo {
            vendor: 0x1234,
            product: 0xB042,
            hid_name: "MX Master 4".into(),
            hidpp_long: true,
        };
        assert!(!is_target(&info, DIRECT_DEVICE));
    }

    #[test]
    fn missing_sysfs_scans_empty() {
        assert!(scan(Path::new("/nonexistent"), Path::new("/dev"), DIRECT_DEVICE).is_empty());
    }
}
