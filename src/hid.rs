//! Minimal hidraw transport: sysfs enumeration and feature-report ioctls.
//!
//! No libudev/hidapi dependency; the K86 vendor interface is a single
//! 64-byte feature report with report id 0, so this is all we need.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

pub const REPORT_SIZE: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HidInfo {
    pub node: PathBuf,
    pub vid: u16,
    pub pid: u16,
    pub name: String,
    /// First usage page declared by the report descriptor.
    pub usage_page: u16,
}

const fn ioc(dir: u32, nr: u32, size: u32) -> libc::c_ulong {
    ((dir << 30) | (size << 16) | ((b'H' as u32) << 8) | nr) as libc::c_ulong
}
const IOC_RW: u32 = 3;
const fn hidiocsfeature(len: u32) -> libc::c_ulong {
    ioc(IOC_RW, 0x06, len)
}
const fn hidiocgfeature(len: u32) -> libc::c_ulong {
    ioc(IOC_RW, 0x07, len)
}

fn parse_uevent(text: &str) -> Option<(u16, u16, String)> {
    let mut ids = None;
    let mut name = String::new();
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("HID_ID=") {
            let mut parts = v.split(':');
            let _bus = parts.next()?;
            let vid = u32::from_str_radix(parts.next()?, 16).ok()? as u16;
            let pid = u32::from_str_radix(parts.next()?, 16).ok()? as u16;
            ids = Some((vid, pid));
        } else if let Some(v) = line.strip_prefix("HID_NAME=") {
            name = v.to_string();
        }
    }
    ids.map(|(v, p)| (v, p, name))
}

/// Reads the first Usage Page item of a report descriptor.
fn first_usage_page(desc: &[u8]) -> u16 {
    let mut i = 0;
    while i < desc.len() {
        let prefix = desc[i];
        let size = match prefix & 0x03 {
            3 => 4,
            n => n as usize,
        };
        if prefix & 0xFC == 0x04 {
            let mut v: u32 = 0;
            for k in 0..size.min(4) {
                v |= (*desc.get(i + 1 + k).unwrap_or(&0) as u32) << (8 * k);
            }
            return v as u16;
        }
        i += 1 + size;
    }
    0
}

/// Whether the interface's report descriptor declares report IDs.
pub fn has_report_ids(info: &HidInfo) -> bool {
    let Some(name) = info.node.file_name() else { return true };
    let desc = fs::read(Path::new("/sys/class/hidraw").join(name).join("device/report_descriptor")).unwrap_or_default();
    let mut i = 0;
    while i < desc.len() {
        let prefix = desc[i];
        if prefix & 0xFC == 0x84 {
            return true;
        }
        i += 1 + match prefix & 0x03 {
            3 => 4,
            n => n as usize,
        };
    }
    false
}

pub fn enumerate() -> Vec<HidInfo> {
    let mut out = Vec::new();
    let Ok(dir) = fs::read_dir("/sys/class/hidraw") else {
        return out;
    };
    for entry in dir.flatten() {
        let sys = entry.path();
        let Ok(uevent) = fs::read_to_string(sys.join("device/uevent")) else {
            continue;
        };
        let Some((vid, pid, name)) = parse_uevent(&uevent) else {
            continue;
        };
        let desc = fs::read(sys.join("device/report_descriptor")).unwrap_or_default();
        out.push(HidInfo {
            node: Path::new("/dev").join(entry.file_name()),
            vid,
            pid,
            name,
            usage_page: first_usage_page(&desc),
        });
    }
    out.sort_by(|a, b| a.node.cmp(&b.node));
    out
}

pub struct HidDevice {
    file: File,
    pub info: HidInfo,
}

impl HidDevice {
    pub fn open(info: &HidInfo) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(&info.node)?;
        Ok(Self { file, info: info.clone() })
    }

    /// Sends feature report 0 with `data` zero-padded to 64 bytes.
    pub fn set_feature(&self, data: &[u8]) -> io::Result<()> {
        let mut buf = [0u8; REPORT_SIZE + 1];
        let n = data.len().min(REPORT_SIZE);
        buf[1..1 + n].copy_from_slice(&data[..n]);
        let rc = unsafe {
            libc::ioctl(
                self.file.as_raw_fd(),
                hidiocsfeature(buf.len() as u32),
                buf.as_mut_ptr(),
            )
        };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Reads feature report 0 (64 bytes, report id stripped).
    pub fn get_feature(&self) -> io::Result<[u8; REPORT_SIZE]> {
        let mut buf = [0u8; REPORT_SIZE + 1];
        let rc = unsafe {
            libc::ioctl(
                self.file.as_raw_fd(),
                hidiocgfeature(buf.len() as u32),
                buf.as_mut_ptr(),
            )
        };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut out = [0u8; REPORT_SIZE];
        out.copy_from_slice(&buf[1..]);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_uevent() {
        let t = "DRIVER=hid-generic\nHID_ID=0003:00003151:00004015\nHID_NAME=ROYUAN Gaming Keyboard\n";
        assert_eq!(
            parse_uevent(t),
            Some((0x3151, 0x4015, "ROYUAN Gaming Keyboard".into()))
        );
    }

    #[test]
    fn usage_page_from_descriptor() {
        let vendor = [0x06, 0xff, 0xff, 0x09, 0x02, 0xa1, 0x01];
        assert_eq!(first_usage_page(&vendor), 0xffff);
        let kbd = [0x05, 0x01, 0x09, 0x06, 0xa1, 0x01];
        assert_eq!(first_usage_page(&kbd), 0x0001);
    }
}
