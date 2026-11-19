//! The MX Master 4 connection shared by every mx4 module.

pub mod device;
pub mod haptics;
pub mod hidraw;

pub use device::{Device, DeviceConfig};
pub use haptics::Haptics;
