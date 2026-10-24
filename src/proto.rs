//! Rongyuan YC3121 keyboard protocol, as used by the Attack Shark K86
//! (device id 1168, `yc3121_sg8967_3m_soc`). Ported from the original
//! driver's `CommonKbYc500` class; command bytes, chunking and timings match.

use std::thread::sleep;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};

use crate::hid::HidDevice;

pub const VID: u16 = 0x3151;
pub const PID: u16 = 0x4015;
pub const DEVICE_ID: u32 = 1168;
/// Key slots in a matrix (4 bytes each).
pub const SLOTS: usize = 128;
pub const MACRO_LEN: usize = 256;
pub const LAYERS: u8 = 3;

/// Settle time the original driver waits after every write ("vendorSleep").
const VENDOR_SLEEP: Duration = Duration::from_millis(500);
const PRE_SEND: Duration = Duration::from_millis(10);
const PRE_READ: Duration = Duration::from_millis(10);
const USERPIC_COMMIT: Duration = Duration::from_millis(2500);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Checksum {
    /// byte[7] = 255 - sum(bytes[0..7])
    Bit7,
    /// byte[8] = 255 - sum(bytes[0..8])
    Bit8,
    None,
}

mod cmd {
    pub const SET_RESET: u8 = 2;
    pub const SET_PROFILE: u8 = 5;
    pub const SET_LEDPARAM: u8 = 7;
    pub const SET_KEYMATRIX: u8 = 9;
    pub const SET_USERPIC: u8 = 12;
    pub const SET_FN: u8 = 16;
    pub const SET_DEBOUNCE: u8 = 17;
    pub const SET_SLEEPTIME: u8 = 18;
    pub const SET_KEYMATRIX_SIMPLE: u8 = 19;
    pub const SET_FN_SIMPLE: u8 = 21;
    pub const SET_MACRO: u8 = 22;
    pub const SET_TFTLCDDATA: u8 = 37;
    pub const SET_OLEDCLOCK: u8 = 40;
    pub const GET_REV: u8 = 128;
    pub const GET_PROFILE: u8 = 133;
    pub const GET_LEDPARAM: u8 = 135;
    pub const GET_KEYMATRIX: u8 = 137;
    pub const GET_MACRO: u8 = 139;
    pub const GET_USERPIC: u8 = 140;
    pub const WHO_AM_I: u8 = 143;
    pub const GET_FN: u8 = 144;
    pub const GET_DEBOUNCE: u8 = 145;
    pub const GET_SLEEPTIME: u8 = 146;
    pub const GET_TFTLCDDATA: u8 = 165;
    pub const SET_FLASHCHIPERASE: u8 = 172;
    pub const GET_OLED_VERSION: u8 = 173;
    pub const BOOT_USB: [u8; 8] = [127, 85, 170, 85, 170, 0, 0, 130];
}

pub fn apply_checksum(data: &mut Vec<u8>, mode: Checksum) {
    if data.len() < 8 {
        data.resize(8, 0);
    }
    let at = match mode {
        Checksum::Bit7 => 7,
        Checksum::Bit8 => 8,
        Checksum::None => return,
    };
    if data.len() <= at {
        data.resize(at + 1, 0);
    }
    let sum: u32 = data[..at].iter().map(|&b| b as u32).sum();
    data[at] = 255 - (sum & 255) as u8;
}

/// Little-endian byte sum used by the bootloader for image verification.
pub fn byte_sum(data: &[u8]) -> u32 {
    data.iter().fold(0u32, |a, &b| a.wrapping_add(b as u32))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
    pub fn from_u32(v: u32) -> Self {
        Self::new((v >> 16) as u8, (v >> 8) as u8, v as u8)
    }
    pub fn to_u32(self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | self.b as u32
    }
}

/// Main backlight effects (`LightList` in the original).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightMode(pub u8);

impl LightMode {
    pub const USER_PICTURE: Self = Self(13);
    pub const MUSIC_FOLLOW3: Self = Self(20);
    pub const SCREEN_COLOR: Self = Self(21);
    pub const MUSIC_FOLLOW2: Self = Self(22);

    /// Internal names from the original driver, used as i18n keys.
    pub fn key(self) -> &'static str {
        match self.0 {
            0 => "LightOff",
            1 => "LightAlwaysOn",
            2 => "LightBreath",
            3 => "LightNeon",
            4 => "LightWave",
            5 => "LightRipple",
            6 => "LightRaindrop",
            7 => "LightSnake",
            8 => "LightPressAction",
            9 => "LightConverage",
            10 => "LightSineWave",
            11 => "LightKaleidoscope",
            12 => "LightLineWave",
            13 => "LightUserPicture",
            14 => "LightLaser",
            15 => "LightCircleWave",
            16 => "LightDazzing",
            17 => "LightRainDown",
            18 => "LightMeteor",
            19 => "LightPressActionOff",
            20 => "LightMusicFollow3",
            21 => "LightScreenColor",
            22 => "LightMusicFollow2",
            23 => "LightTrain",
            24 => "LightFireWorks",
            66 => "LightUserColor",
            _ => "LightUnknown",
        }
    }

    pub fn is_music(self) -> bool {
        self == Self::MUSIC_FOLLOW2 || self == Self::MUSIC_FOLLOW3
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightSetting {
    pub mode: LightMode,
    /// Effect-specific sub option (direction etc.), high nibble of byte 4.
    pub option: u8,
    pub bright: u8,
    /// UI speed; the wire value is `MAX_SPEED - speed`.
    pub speed: u8,
    pub rgb: Rgb,
    /// Rainbow ("dazzle") colours instead of the fixed rgb.
    pub dazzle: bool,
}

const MAX_SPEED: u8 = 5;
const DAZZLE: u8 = 8;
const NORMAL: u8 = 7;
/// White is sent as FAFFFA (the LEDs over-saturate on pure white).
const WIRE_WHITE: u32 = 0xFAFFFA;
/// Colours selected by a non-normal low nibble (`COMMONCOLOR`).
const COMMON_COLOR: [u32; 7] = [0xFF0000, 0xFF8000, 0xFFFF00, 0x00FF00, 0x00FFFF, 0x0000FF, 0xFF00FF];

impl LightSetting {
    fn encode(&self) -> Vec<u8> {
        let mut e = vec![0u8; 64];
        e[0] = cmd::SET_LEDPARAM;
        let low = if self.dazzle { DAZZLE } else { NORMAL };
        let mut opt = (self.option << 4) | low;
        if self.mode == LightMode::USER_PICTURE {
            opt = self.option << 4;
        }
        if self.mode.is_music() {
            opt = (self.option << 4) | if self.dazzle { 0 } else { 4 };
        }
        if self.mode == LightMode::SCREEN_COLOR {
            opt = 0;
        }
        e[1] = self.mode.0;
        e[2] = MAX_SPEED.saturating_sub(self.speed);
        e[3] = self.bright;
        e[4] = opt;
        let mut c = self.rgb.to_u32();
        if c == 0xFFFFFF {
            c = WIRE_WHITE;
        }
        let rgb = Rgb::from_u32(c);
        e[5] = rgb.r;
        e[6] = rgb.g;
        e[7] = rgb.b;
        if self.mode == LightMode::USER_PICTURE {
            e[5] = 0;
            e[6] = 200;
            e[7] = 200;
        }
        e
    }

    fn decode(e: &[u8]) -> Self {
        let mode = LightMode(e[1]);
        let opt = e[4];
        let low = opt & 15;
        let mut rgb = Rgb::new(e[5], e[6], e[7]).to_u32();
        if low != DAZZLE && low != NORMAL {
            if let Some(&c) = COMMON_COLOR.get(low as usize) {
                rgb = c;
            }
        }
        if rgb == WIRE_WHITE {
            rgb = 0xFFFFFF;
        }
        let dazzle = if mode.is_music() { low == 0 } else { low == DAZZLE };
        Self {
            mode,
            option: opt >> 4,
            bright: e[3],
            speed: MAX_SPEED.saturating_sub(e[2]),
            rgb: Rgb::from_u32(rgb),
            dazzle,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SleepTimes {
    pub bt: u16,
    pub rf24: u16,
    pub deep_bt: u16,
    pub deep_rf24: u16,
}

impl Default for SleepTimes {
    fn default() -> Self {
        Self { bt: 120, rf24: 120, deep_bt: 28 * 60, deep_rf24: 28 * 60 }
    }
}

/// One screen upload unit (`getTFTLCDDataRGBImg` / `setTFTLCDDataRGBImg`).
pub struct ScreenFrame<'a> {
    pub current_frame: u8,
    pub frame_num: u8,
    pub frame_delay: u8,
    pub left: u16,
    pub top: u16,
    pub right: u16,
    pub bottom: u16,
    pub data: &'a [u8],
}

pub struct K86 {
    dev: HidDevice,
}

impl K86 {
    pub fn new(dev: HidDevice) -> Self {
        Self { dev }
    }

    pub fn node(&self) -> &std::path::Path {
        &self.dev.info.node
    }

    // ---- transport -----------------------------------------------------

    fn send(&self, payload: &[u8], mode: Checksum, pre: Duration) -> Result<()> {
        if !pre.is_zero() {
            sleep(pre);
        }
        let mut data = payload.to_vec();
        apply_checksum(&mut data, mode);
        self.dev.set_feature(&data)?;
        Ok(())
    }

    fn read(&self, pre: Duration) -> Result<[u8; 64]> {
        if !pre.is_zero() {
            sleep(pre);
        }
        Ok(self.dev.get_feature()?)
    }

    fn query(&self, payload: &[u8]) -> Result<[u8; 64]> {
        self.send(payload, Checksum::Bit7, PRE_SEND)?;
        self.read(PRE_READ)
    }

    fn query_slow(&self, payload: &[u8], read_delay: Duration) -> Result<[u8; 64]> {
        self.send(payload, Checksum::Bit7, PRE_SEND)?;
        self.read(read_delay)
    }

    fn settle(&self) {
        sleep(VENDOR_SLEEP);
    }

    /// Sends `[header(8) | 56-byte chunk]` pages with `header[page_at] = page`.
    fn send_pages(&self, header: [u8; 8], page_at: usize, data: &[u8], pages: usize, pre: Duration) -> Result<()> {
        for page in 0..pages {
            let mut h = header;
            h[page_at] = page as u8;
            let mut msg = h.to_vec();
            let start = (page * 56).min(data.len());
            let end = ((page + 1) * 56).min(data.len());
            msg.extend_from_slice(&data[start..end]);
            msg.resize(64, 0);
            self.send(&msg, Checksum::Bit7, pre)?;
        }
        Ok(())
    }

    fn read_pages(&self, cmd_byte: u8, arg: u8, pages: usize) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(pages * 64);
        for page in 0..pages {
            let mut n = [0u8; 64];
            n[0] = cmd_byte;
            n[1] = arg;
            n[2] = page as u8;
            out.extend_from_slice(&self.query(&n)?);
        }
        Ok(out)
    }

    // ---- identity --------------------------------------------------------

    pub fn who_am_i(&self) -> Result<u32> {
        for _ in 0..3 {
            let r = self.query(&[cmd::WHO_AM_I])?;
            if r[0] == cmd::WHO_AM_I {
                return Ok(u32::from_le_bytes([r[1], r[2], r[3], r[4]]));
            }
        }
        bail!("keyboard did not identify itself")
    }

    pub fn usb_version(&self) -> Result<u16> {
        let t = self.query(&[cmd::GET_REV])?;
        Ok(((t[2] as u16) << 8) | t[1] as u16)
    }

    /// Screen (OLED/TFT) controller and resource flash versions.
    pub fn oled_version(&self) -> Result<Option<(u16, u16)>> {
        let t = self.query_slow(&[cmd::GET_OLED_VERSION], Duration::from_millis(100))?;
        if t[0] != cmd::GET_OLED_VERSION {
            return Ok(None);
        }
        Ok(Some((
            u16::from_le_bytes([t[1], t[2]]),
            u16::from_le_bytes([t[3], t[4]]),
        )))
    }

    // ---- profiles, options --------------------------------------------

    pub fn profile(&self) -> Result<u8> {
        Ok(self.query(&[cmd::GET_PROFILE])?[1])
    }

    pub fn set_profile(&self, p: u8) -> Result<()> {
        self.send(&[cmd::SET_PROFILE, p], Checksum::Bit7, PRE_SEND)?;
        self.settle();
        Ok(())
    }

    pub fn debounce(&self) -> Result<u8> {
        Ok(self.query(&[cmd::GET_DEBOUNCE])?[2])
    }

    pub fn set_debounce(&self, ms: u8) -> Result<()> {
        self.send(&[cmd::SET_DEBOUNCE, 0, ms], Checksum::Bit7, PRE_SEND)?;
        self.settle();
        Ok(())
    }

    pub fn sleep_times(&self) -> Result<SleepTimes> {
        let e = self.query(&[cmd::GET_SLEEPTIME])?;
        let w = |i: usize| ((e[i + 1] as u16) << 8) | e[i] as u16;
        Ok(SleepTimes { bt: w(1), rf24: w(3), deep_bt: w(5), deep_rf24: w(7) })
    }

    pub fn set_sleep_times(&self, t: SleepTimes) -> Result<()> {
        let mut e = vec![0u8; 64];
        e[0] = cmd::SET_SLEEPTIME;
        e[8..10].copy_from_slice(&t.bt.to_le_bytes());
        e[10..12].copy_from_slice(&t.rf24.to_le_bytes());
        e[12..14].copy_from_slice(&t.deep_bt.to_le_bytes());
        e[14..16].copy_from_slice(&t.deep_rf24.to_le_bytes());
        self.send(&e, Checksum::Bit7, PRE_SEND)?;
        self.settle();
        Ok(())
    }

    /// Factory reset; the keyboard needs ~4 s afterwards.
    pub fn reset(&self) -> Result<()> {
        self.send(&[cmd::SET_RESET], Checksum::Bit7, PRE_SEND)?;
        sleep(Duration::from_secs(4));
        Ok(())
    }

    // ---- lighting --------------------------------------------------------

    pub fn light(&self) -> Result<LightSetting> {
        let mut t = [0u8; 64];
        t[0] = cmd::GET_LEDPARAM;
        Ok(LightSetting::decode(&self.query(&t)?))
    }

    pub fn set_light(&self, s: &LightSetting) -> Result<()> {
        self.send(&s.encode(), Checksum::Bit8, PRE_SEND)?;
        self.settle();
        Ok(())
    }

    /// Per-key colours for the user-picture effect, indexed by matrix slot.
    pub fn user_picture(&self) -> Result<Vec<Rgb>> {
        let mut raw = Vec::with_capacity(6 * 64);
        for page in 0..6u8 {
            let mut e = [0u8; 8];
            e[0] = cmd::GET_USERPIC;
            e[2] = page;
            raw.extend_from_slice(&self.query(&e)?);
        }
        Ok(raw.chunks(3).take(SLOTS).map(|c| Rgb::new(c[0], c[1], c[2])).collect())
    }

    pub fn set_user_picture(&self, colors: &[Rgb]) -> Result<()> {
        let mut data = vec![0u8; SLOTS * 3];
        for (i, c) in colors.iter().take(SLOTS).enumerate() {
            data[i * 3] = c.r;
            data[i * 3 + 1] = c.g;
            data[i * 3 + 2] = c.b;
        }
        let total: u16 = 64 * ((SLOTS * 3 / 64) as u16);
        let t = total.to_le_bytes();
        let header = [cmd::SET_USERPIC, 0, t[0], t[1], 0, 0, 0, 0];
        self.send_pages(header, 4, &data, 7, PRE_SEND)?;
        // The keyboard commits the picture to flash for ~1.5 s; reads in that
        // window return stale pages and further writes can reset it.
        sleep(USERPIC_COMMIT);
        Ok(())
    }

    // ---- key matrices ----------------------------------------------------

    pub fn key_matrix(&self, layer: u8) -> Result<Vec<u8>> {
        self.read_pages(cmd::GET_KEYMATRIX, layer, 8)
    }

    /// Writes a whole layer. Lands on the active profile (firmware V114).
    pub fn set_key_matrix(&self, layer: u8, matrix: &[u8]) -> Result<()> {
        let header = [cmd::SET_KEYMATRIX, layer, 248, 1, 0, 0, 0, 0];
        self.send_pages(header, 4, matrix, 9, PRE_SEND)?;
        self.settle();
        Ok(())
    }

    pub fn fn_matrix(&self, layer: u8) -> Result<Vec<u8>> {
        self.read_pages(cmd::GET_FN, layer, 8)
    }

    pub fn set_fn_matrix(&self, layer: u8, matrix: &[u8]) -> Result<()> {
        let header = [cmd::SET_FN, layer, 248, 1, 0, 0, 0, 0];
        self.send_pages(header, 4, matrix, 9, PRE_SEND)?;
        self.settle();
        Ok(())
    }

    /// Rewrites a single key (`setKeyConfigSimple`). Like `set_key_matrix`,
    /// the firmware applies it to the *active* profile; see `device.rs`.
    pub fn set_key(&self, layer: u8, slot: u8, value: [u8; 4]) -> Result<()> {
        if slot as usize >= SLOTS {
            return Err(anyhow!("slot out of range"));
        }
        let mut a = vec![0u8; 64];
        a[0] = cmd::SET_KEYMATRIX_SIMPLE;
        a[1] = layer;
        a[2] = slot;
        a[8..12].copy_from_slice(&value);
        self.send(&a, Checksum::Bit7, PRE_SEND)?;
        self.settle();
        Ok(())
    }

    /// Rewrites a single Fn-layer key (`setFnKeyConfigSimple`).
    pub fn set_fn_key(&self, layer: u8, slot: u8, value: [u8; 4]) -> Result<()> {
        let mut a = vec![0u8; 64];
        a[0] = cmd::SET_FN_SIMPLE;
        a[1] = layer;
        a[2] = slot;
        a[8..12].copy_from_slice(&value);
        self.send(&a, Checksum::Bit7, PRE_SEND)?;
        self.settle();
        Ok(())
    }

    // ---- macros ----------------------------------------------------------

    /// Raw 256-byte macro buffer (repeat count + events).
    pub fn macro_raw(&self, index: u8) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        for page in 0..4u8 {
            let mut n = [0u8; 64];
            n[0] = cmd::GET_MACRO;
            n[1] = index;
            n[2] = page;
            let s = self.query(&n)?;
            out.extend_from_slice(&s);
            if s.windows(4).any(|w| w == [0, 0, 0, 0]) {
                break;
            }
        }
        Ok(out)
    }

    pub fn set_macro_raw(&self, index: u8, buf: &[u8]) -> Result<()> {
        let mut data = buf.to_vec();
        data.resize(5 * 56, 0);
        let pages = (0..5).filter(|&p| data[p * 56..(p + 1) * 56].iter().any(|&b| b != 0)).count();
        for page in 0..pages {
            let mut msg = vec![
                cmd::SET_MACRO,
                index,
                page as u8,
                56,
                (page == pages - 1) as u8,
                0,
                0,
                0,
            ];
            msg.extend_from_slice(&data[page * 56..(page + 1) * 56]);
            self.send(&msg, Checksum::Bit7, PRE_SEND)?;
        }
        self.settle();
        Ok(())
    }

    // ---- screen ------------------------------------------------------------

    /// Announces a 16-bit screen upload; returns whether the keyboard is ready.
    pub fn screen_begin(&self, f: &ScreenFrame, slot: u8) -> Result<bool> {
        let mut i = [0u8; 64];
        let s = f.data.len() as u32;
        i[0] = cmd::GET_TFTLCDDATA;
        i[1] = f.current_frame;
        i[2] = f.frame_num;
        i[3] = f.frame_delay;
        i[4] = s as u8;
        i[5] = (s >> 8) as u8;
        i[8] = f.left as u8;
        i[9] = f.top as u8;
        i[10] = f.right as u8;
        i[11] = f.bottom as u8;
        i[12] = (f.left >> 8) as u8;
        i[13] = (f.top >> 8) as u8;
        i[14] = (f.right >> 8) as u8;
        i[15] = (f.bottom >> 8) as u8;
        i[16] = (s >> 16) as u8;
        i[17] = (s >> 24) as u8;
        i[18] = slot;
        let delay = Duration::from_millis(100);
        let mut a = self.query_slow(&i, delay).ok();
        let mut tries = 0;
        while a.map_or(true, |r| r[1] == 0) && tries < 10 {
            sleep(delay);
            tries += 1;
            a = self.query_slow(&i, delay).ok();
        }
        sleep(delay);
        Ok(a.is_some_and(|r| r[1] == 1))
    }

    pub fn screen_data(&self, f: &ScreenFrame, mut progress: impl FnMut(f32)) -> Result<()> {
        let chunks = f.data.len().div_ceil(56);
        for c in 0..chunks {
            let part = &f.data[c * 56..((c + 1) * 56).min(f.data.len())];
            let mut msg = vec![
                cmd::SET_TFTLCDDATA,
                f.current_frame,
                f.frame_num,
                f.frame_delay,
                c as u8,
                (c >> 8) as u8,
                part.len() as u8,
                0,
            ];
            msg.extend_from_slice(part);
            msg.resize(64, 0);
            self.send(&msg, Checksum::Bit7, Duration::from_millis(1))?;
            progress(c as f32 / chunks as f32);
        }
        progress(1.0);
        Ok(())
    }

    pub fn set_clock(&self, now: chrono::NaiveDateTime) -> Result<()> {
        use chrono::{Datelike, Timelike};
        let mut t = vec![0u8; 64];
        t[0] = cmd::SET_OLEDCLOCK;
        let y = now.year() as u16;
        t[8] = (y >> 8) as u8;
        t[9] = y as u8;
        t[10] = now.month() as u8;
        t[11] = now.day() as u8;
        t[12] = now.hour() as u8;
        t[13] = now.minute() as u8;
        t[14] = now.second() as u8;
        self.send(&t, Checksum::Bit7, Duration::ZERO)
    }

    /// Erases the screen's resource flash. Returns the keyboard's acknowledgement.
    pub fn erase_screen_flash(&self) -> Result<bool> {
        let mut t = [0u8; 64];
        t[0] = cmd::SET_FLASHCHIPERASE;
        let e = self.query(&t)?;
        Ok(e[0] == cmd::SET_FLASHCHIPERASE && e[1..5] == [170, 170, 85, 85])
    }

    // ---- firmware ----------------------------------------------------------

    /// Asks the keyboard to reboot into its USB bootloader.
    pub fn enter_bootloader(&self) -> Result<()> {
        self.send(&cmd::BOOT_USB, Checksum::Bit7, PRE_SEND)
    }

    pub fn raw_send(&self, data: &[u8], mode: Checksum, pre: Duration) -> Result<()> {
        self.send(data, mode, pre)
    }

    pub fn raw_read(&self, pre: Duration) -> Result<[u8; 64]> {
        self.read(pre)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_bit7_matches_device_replies() {
        // GET_REV request as captured from the keyboard.
        let mut d = vec![cmd::GET_REV];
        apply_checksum(&mut d, Checksum::Bit7);
        assert_eq!(d, vec![0x80, 0, 0, 0, 0, 0, 0, 0x7f]);
    }

    #[test]
    fn light_round_trip() {
        // Captured: 87 11 01 00 08 b4 b4 b4 (RainDown, rainbow).
        let reply = [0x87, 0x11, 0x01, 0x00, 0x08, 0xb4, 0xb4, 0xb4];
        let s = LightSetting::decode(&reply);
        assert_eq!(s.mode, LightMode(17));
        assert_eq!(s.speed, 4);
        assert!(s.dazzle);
        assert_eq!(s.rgb, Rgb::new(0xb4, 0xb4, 0xb4));
        let e = s.encode();
        assert_eq!(&e[1..8], &reply[1..8]);
    }

    #[test]
    fn white_is_sent_as_fafffa() {
        let s = LightSetting {
            mode: LightMode(1),
            option: 0,
            bright: 4,
            speed: 3,
            rgb: Rgb::from_u32(0xFFFFFF),
            dazzle: false,
        };
        let e = s.encode();
        assert_eq!(&e[5..8], &[0xFA, 0xFF, 0xFA]);
        assert_eq!(e[4], NORMAL);
        assert_eq!(LightSetting::decode(&e).rgb, Rgb::from_u32(0xFFFFFF));
    }
}

/// Round trips against a connected K86. Each test restores what it changed.
/// Run with `cargo test -- --ignored --test-threads=1`.
#[cfg(test)]
mod hardware {
    use super::*;
    use crate::keymap::{self, Macro, MacroEvent, MacroInput};

    fn open() -> K86 {
        let info = crate::hid::enumerate()
            .into_iter()
            .find(|d| d.vid == VID && d.pid == PID && d.usage_page == 0xFFFF)
            .expect("K86 not connected");
        let kb = K86::new(HidDevice::open(&info).expect("open"));
        assert_eq!(kb.who_am_i().unwrap(), DEVICE_ID);
        kb
    }

    #[test]
    #[ignore]
    fn light_write_read_restore() {
        let kb = open();
        let orig = kb.light().unwrap();
        let mut l = orig;
        l.bright = if orig.bright == 2 { 3 } else { 2 };
        l.speed = if orig.speed == 1 { 2 } else { 1 };
        kb.set_light(&l).unwrap();
        let got = kb.light().unwrap();
        kb.set_light(&orig).unwrap();
        assert_eq!((got.mode, got.bright, got.speed, got.dazzle), (l.mode, l.bright, l.speed, l.dazzle));
        assert_eq!(kb.light().unwrap(), orig);
    }

    #[test]
    #[ignore]
    fn key_write_read_restore() {
        let kb = open();
        let layer = kb.profile().unwrap();
        let slot = keymap::slot_of_hid(4, 0).unwrap(); // A
        let before = kb.key_matrix(layer).unwrap();
        let orig = keymap::slot(&before, slot);
        kb.set_key(layer, slot as u8, [0, 0, 5, 0]).unwrap(); // -> B
        let after = kb.key_matrix(layer).unwrap();
        kb.set_key(layer, slot as u8, orig).unwrap();
        assert_eq!(keymap::slot(&after, slot), [0, 0, 5, 0]);
        // Nothing else moved.
        for i in (0..SLOTS).filter(|&i| i != slot) {
            assert_eq!(keymap::slot(&after, i), keymap::slot(&before, i), "slot {i}");
        }
        assert_eq!(kb.key_matrix(layer).unwrap(), before);
    }

    #[test]
    #[ignore]
    fn full_matrix_write_other_profile() {
        let kb = open();
        let active = kb.profile().unwrap();
        let layer = (active + 1) % LAYERS;
        let slot = keymap::slot_of_hid(4, 0).unwrap();
        let before: Vec<Vec<u8>> = (0..LAYERS).map(|l| kb.key_matrix(l).unwrap()).collect();
        kb.set_profile(layer).unwrap();
        let mut m = before[layer as usize].clone();
        keymap::set_slot(&mut m, slot, [0, 0, 5, 0]);
        kb.set_key_matrix(layer, &m).unwrap();
        let after: Vec<Vec<u8>> = (0..LAYERS).map(|l| kb.key_matrix(l).unwrap()).collect();
        kb.set_key_matrix(layer, &before[layer as usize]).unwrap();
        kb.set_profile(active).unwrap();
        // Only the targeted profile changed, and reads are per profile.
        assert_eq!(keymap::slot(&after[layer as usize], slot), [0, 0, 5, 0]);
        for l in (0..LAYERS).filter(|&l| l != layer) {
            assert_eq!(after[l as usize], before[l as usize], "profile {l}");
        }
        for l in 0..LAYERS {
            assert_eq!(kb.key_matrix(l).unwrap(), before[l as usize]);
        }
        assert_eq!(kb.profile().unwrap(), active);
    }

    #[test]
    #[ignore]
    fn fn_key_write_read_restore() {
        let kb = open();
        let slot = keymap::slot_of_hid(4, 0).unwrap();
        let before = kb.fn_matrix(0).unwrap();
        kb.set_fn_key(0, slot as u8, [3, 0, 233, 0]).unwrap(); // Fn+A -> Volume Up
        let after = kb.fn_matrix(0).unwrap();
        kb.set_fn_key(0, slot as u8, keymap::slot(&before, slot)).unwrap();
        assert_eq!(keymap::slot(&after, slot), [3, 0, 233, 0]);
        assert_eq!(kb.fn_matrix(0).unwrap(), before);
    }

    #[test]
    #[ignore]
    fn user_picture_write_read_restore() {
        let kb = open();
        let light = kb.light().unwrap();
        let mut l = light;
        l.mode = LightMode::USER_PICTURE;
        l.option = 2;
        kb.set_light(&l).unwrap();
        let orig = kb.user_picture().unwrap();
        let test: Vec<Rgb> = (0..SLOTS).map(|i| Rgb::new(i as u8, 255 - i as u8, 7)).collect();
        kb.set_user_picture(&test).unwrap();
        let got = kb.user_picture().unwrap();
        kb.set_user_picture(&orig).unwrap();
        kb.set_light(&light).unwrap();
        assert_eq!(&got[..SLOTS], &test[..]);
        let now = kb.light().unwrap();
        // In rainbow mode the keyboard may echo white as a dimmed grey.
        assert_eq!((now.mode, now.option, now.bright, now.speed, now.dazzle), (light.mode, light.option, light.bright, light.speed, light.dazzle));
    }

    #[test]
    #[ignore]
    fn macro_write_read() {
        let kb = open();
        let m = Macro {
            repeat: 2,
            events: vec![
                MacroEvent::Press { input: MacroInput::Key(4), down: true, delay: 30 },
                MacroEvent::Press { input: MacroInput::Key(4), down: false, delay: 400 },
            ],
        };
        kb.set_macro_raw(49, &m.encode()).unwrap();
        let back = Macro::decode(&kb.macro_raw(49).unwrap());
        assert_eq!(back, m);
    }

    #[test]
    #[ignore]
    fn sleep_and_debounce_restore() {
        let kb = open();
        let (s, d) = (kb.sleep_times().unwrap(), kb.debounce().unwrap());
        kb.set_debounce(if d == 5 { 6 } else { 5 }).unwrap();
        let d2 = kb.debounce().unwrap();
        kb.set_debounce(d).unwrap();
        assert_eq!(d2, if d == 5 { 6 } else { 5 });
        kb.set_sleep_times(s).unwrap();
        assert_eq!(kb.sleep_times().unwrap(), s);
        assert_eq!(kb.debounce().unwrap(), d);
    }
}

