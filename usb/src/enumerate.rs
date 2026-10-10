//! Finding devices: what sysfs lists under `/sys/bus/usb/devices`.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::device::Device;
use crate::error::Error;

/// Where sysfs lists USB devices.
const SYSFS: &str = "/sys/bus/usb/devices";
/// Where usbfs puts their nodes.
const NODES: &str = "/dev/bus/usb";

/// Every USB device attached, root hubs excepted, in port order.
///
/// # Errors
///
/// Only if sysfs cannot be read; a system without USB lists nothing.
pub fn devices() -> Result<Vec<DeviceInfo>, Error> {
    devices_in(Path::new(SYSFS), Path::new(NODES))
}

/// [`devices`], reading sysfs at `sysfs` and naming nodes under `nodes`.
pub(crate) fn devices_in(sysfs: &Path, nodes: &Path) -> Result<Vec<DeviceInfo>, Error> {
    let entries = match fs::read_dir(sysfs) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut found: Vec<DeviceInfo> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| DeviceInfo::read(&entry.path(), nodes))
        .collect();
    found.sort_by(|a, b| a.port_path.cmp(&b.port_path));
    Ok(found)
}

/// A device as found, before it is opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceInfo {
    /// Its sysfs directory.
    sysfs: PathBuf,

    /// Its usbfs node.
    node: PathBuf,

    /// `idVendor`.
    vendor_id: u16,

    /// `idProduct`.
    product_id: u16,

    /// Where it is plugged in.
    port_path: PortPath,

    /// Its serial number string, if it has one.
    serial: Option<String>,
}

impl DeviceInfo {
    /// The device in sysfs directory `dir`, if it is one: interfaces (`1-1:1.0`) and root
    /// hubs (`usb1`) are not.
    fn read(dir: &Path, nodes: &Path) -> Option<Self> {
        let name = dir.file_name()?.to_str()?;
        let port_path = PortPath::parse(name)?;
        let attribute = |file: &str| fs::read_to_string(dir.join(file)).ok();
        let hex = |file: &str| u16::from_str_radix(attribute(file)?.trim(), 16).ok();
        let number = |file: &str| attribute(file)?.trim().parse::<u16>().ok();
        let node = nodes.join(format!("{:03}/{:03}", number("busnum")?, number("devnum")?));
        Some(Self {
            sysfs: dir.to_owned(),
            node,
            vendor_id: hex("idVendor")?,
            product_id: hex("idProduct")?,
            port_path,
            serial: attribute("serial").map(|serial| serial.trim().to_owned()),
        })
    }

    /// The vendor ID.
    #[must_use]
    pub fn vendor_id(&self) -> u16 {
        self.vendor_id
    }

    /// The product ID.
    #[must_use]
    pub fn product_id(&self) -> u16 {
        self.product_id
    }

    /// Where the device is plugged in; stays the same when it re-enumerates.
    #[must_use]
    pub fn port_path(&self) -> &PortPath {
        &self.port_path
    }

    /// The serial number string, if the device has one.
    #[must_use]
    pub fn serial(&self) -> Option<&str> {
        self.serial.as_deref()
    }

    /// Opens the device.
    ///
    /// # Errors
    ///
    /// [`Error::Access`] without write access to its node, [`Error::Disconnected`] if it has
    /// gone.
    pub fn open(&self) -> Result<Device, Error> {
        Device::open(self.clone())
    }

    /// The sysfs directory.
    pub(crate) fn sysfs(&self) -> &Path {
        &self.sysfs
    }

    /// The usbfs node.
    pub(crate) fn node(&self) -> &Path {
        &self.node
    }

    /// The device now at this one's port, after it re-enumerated.
    pub(crate) fn find_again(&self) -> Option<Self> {
        let sysfs = self.sysfs.parent()?;
        let nodes = self.node.parent()?.parent()?;
        devices_in(sysfs, nodes)
            .ok()?
            .into_iter()
            .find(|found| found.port_path == self.port_path)
    }
}

/// Where a device is plugged in: its bus and the hub ports leading to it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PortPath {
    /// The bus number.
    bus: u8,

    /// The port on each hub from the root, outermost last.
    ports: Vec<u8>,
}

impl PortPath {
    /// Parses a sysfs device name such as `1-4.2`.
    fn parse(name: &str) -> Option<Self> {
        let (bus, ports) = name.split_once('-')?;
        if ports.contains(':') {
            return None;
        }
        let ports = ports
            .split('.')
            .map(|port| port.parse().ok())
            .collect::<Option<Vec<u8>>>()?;
        Some(Self {
            bus: bus.parse().ok()?,
            ports,
        })
    }

    /// The bus number.
    #[must_use]
    pub fn bus(&self) -> u8 {
        self.bus
    }

    /// The hub ports from the root.
    #[must_use]
    pub fn ports(&self) -> &[u8] {
        &self.ports
    }
}

impl fmt::Display for PortPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-", self.bus)?;
        for (i, port) in self.ports.iter().enumerate() {
            if i > 0 {
                f.write_str(".")?;
            }
            write!(f, "{port}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(root: &Path, name: &str, ids: (&str, &str), numbers: (u8, u8)) {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("idVendor"), format!("{}\n", ids.0)).unwrap();
        fs::write(dir.join("idProduct"), format!("{}\n", ids.1)).unwrap();
        fs::write(dir.join("busnum"), format!("{}\n", numbers.0)).unwrap();
        fs::write(dir.join("devnum"), format!("{}\n", numbers.1)).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore = "reads the filesystem")]
    fn devices_are_listed_by_port_without_hubs_roots_or_interfaces() {
        let sysfs = tempfile::tempdir().unwrap();
        device(sysfs.path(), "1-4.2", ("3727", "1001"), (1, 9));
        device(sysfs.path(), "1-1", ("0bda", "5411"), (1, 2));
        device(sysfs.path(), "usb1", ("1d6b", "0002"), (1, 1));
        fs::create_dir_all(sysfs.path().join("1-4.2:1.0")).unwrap();
        fs::write(sysfs.path().join("1-4.2/serial"), "ABC123\n").unwrap();

        let found = devices_in(sysfs.path(), Path::new("/dev/bus/usb")).unwrap();
        let names: Vec<String> = found.iter().map(|d| d.port_path().to_string()).collect();
        assert_eq!(names, ["1-1", "1-4.2"]);
        let usdr = &found[1];
        assert_eq!((usdr.vendor_id(), usdr.product_id()), (0x3727, 0x1001));
        assert_eq!(usdr.node(), Path::new("/dev/bus/usb/001/009"));
        assert_eq!(usdr.serial(), Some("ABC123"));
        assert_eq!(found[0].serial(), None);
    }

    #[test]
    #[cfg_attr(miri, ignore = "reads the filesystem")]
    fn a_missing_usb_bus_lists_nothing() {
        let found = devices_in(Path::new("/nonexistent/usb"), Path::new("/dev/bus/usb"));
        assert_eq!(found.unwrap(), []);
    }

    #[test]
    fn port_paths_round_trip_through_their_sysfs_names() {
        for name in ["1-1", "3-10.4.2"] {
            assert_eq!(PortPath::parse(name).unwrap().to_string(), name);
        }
        assert!(
            PortPath::parse("1-1:1.0").is_none(),
            "an interface is not a device"
        );
        assert!(
            PortPath::parse("usb3").is_none(),
            "a root hub is not listed"
        );
    }
}
