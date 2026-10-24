//! Key action codec (4 bytes per matrix slot) and macro byte format.
//!
//! Mirrors the original driver's `Zi.configToMatrix` / `matrixToConfigs` /
//! `changeArrToConfig` and `Ca.macroEventToByte` / `buffToMacroEvents`.
//! Anything not understood is kept verbatim as `Raw`, so round trips are lossless.

use crate::defaults::{DEFAULT_FN_MATRIX, DEFAULT_MATRIX};
use crate::proto::SLOTS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Forward,
    Back,
    WheelForward,
    WheelBack,
    WheelLeft,
    WheelRight,
    ScrollUp,
    ScrollDown,
}

impl MouseButton {
    pub const ALL: [MouseButton; 11] = [
        Self::Left,
        Self::Right,
        Self::Middle,
        Self::Back,
        Self::Forward,
        Self::ScrollUp,
        Self::ScrollDown,
        Self::WheelForward,
        Self::WheelBack,
        Self::WheelLeft,
        Self::WheelRight,
    ];

    /// The original `ba` table.
    pub fn bytes(self) -> [u8; 4] {
        match self {
            Self::Left => [1, 0, 240, 0],
            Self::Right => [1, 0, 241, 0],
            Self::Middle => [1, 0, 242, 0],
            Self::Forward => [1, 0, 244, 0],
            Self::Back => [1, 0, 243, 0],
            Self::WheelForward => [1, 0, 247, 0],
            Self::WheelBack => [1, 0, 248, 0],
            Self::WheelLeft => [1, 0, 245, 0],
            Self::WheelRight => [1, 0, 246, 0],
            Self::ScrollUp => [1, 0, 245, 1],
            Self::ScrollDown => [1, 0, 245, 255],
        }
    }

    pub fn from_bytes(b: [u8; 4]) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.bytes() == b)
    }

    /// Byte used for this button inside macros.
    pub fn macro_code(self) -> Option<u8> {
        match self {
            Self::Left => Some(240),
            Self::Right => Some(241),
            Self::Middle => Some(242),
            Self::Back => Some(243),
            Self::Forward => Some(244),
            _ => None,
        }
    }

    pub fn from_macro_code(c: u8) -> Option<Self> {
        match c {
            240 => Some(Self::Left),
            241 => Some(Self::Right),
            242 => Some(Self::Middle),
            243 => Some(Self::Back),
            244 => Some(Self::Forward),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MacroMode {
    /// Plays the macro N times.
    RepeatTimes,
    /// Toggles playback on/off with the key.
    OnOff,
    /// Repeats while held.
    WhileHeld,
}

impl MacroMode {
    pub const ALL: [MacroMode; 3] = [Self::RepeatTimes, Self::WhileHeld, Self::OnOff];

    fn byte(self) -> u8 {
        match self {
            Self::RepeatTimes => 0,
            Self::OnOff => 1,
            Self::WhileHeld => 2,
        }
    }

    fn from_byte(b: u8) -> Self {
        match b {
            1 => Self::OnOff,
            2 => Self::WhileHeld,
            _ => Self::RepeatTimes,
        }
    }
}

/// Named special functions from the original `mv` table.
pub struct Function {
    pub key: &'static str,
    pub bytes: [u8; 4],
}

pub const FUNCTIONS: &[Function] = &[
    Function { key: "上一曲", bytes: [3, 0, 182, 0] },
    Function { key: "下一曲", bytes: [3, 0, 181, 0] },
    Function { key: "停止", bytes: [3, 0, 183, 0] },
    Function { key: "播放暂停", bytes: [3, 0, 205, 0] },
    Function { key: "播放器", bytes: [3, 0, 131, 1] },
    Function { key: "静音", bytes: [3, 0, 226, 0] },
    Function { key: "音量减", bytes: [3, 0, 234, 0] },
    Function { key: "音量加", bytes: [3, 0, 233, 0] },
    Function { key: "计算器", bytes: [3, 0, 146, 1] },
    Function { key: "fn", bytes: [10, 1, 0, 0] },
    Function { key: "邮件", bytes: [3, 0, 138, 1] },
    Function { key: "我的电脑", bytes: [3, 0, 148, 1] },
    Function { key: "搜索", bytes: [3, 0, 33, 2] },
    Function { key: "主页", bytes: [3, 0, 35, 2] },
    Function { key: "亮度减", bytes: [3, 0, 112, 0] },
    Function { key: "亮度加", bytes: [3, 0, 111, 0] },
    Function { key: "呼出siri", bytes: [18, 0, 227, 44] },
    Function { key: "系统电源", bytes: [2, 0, 129, 0] },
    Function { key: "系统睡眠", bytes: [2, 0, 130, 0] },
    Function { key: "电脑唤醒", bytes: [2, 0, 131, 0] },
    Function { key: "(", bytes: [0, 0, 229, 38] },
    Function { key: ")", bytes: [0, 0, 229, 39] },
    Function { key: "{", bytes: [0, 225, 47, 0] },
    Function { key: "}", bytes: [0, 225, 48, 0] },
    Function { key: "切换输入法", bytes: [0, 224, 44, 0] },
    Function { key: "缩小", bytes: [0, 0, 227, 45] },
    Function { key: "放大", bytes: [0, 0, 227, 46] },
    Function { key: "AI function", bytes: [0, 227, 225, 114] },
    Function { key: "麦克风开关", bytes: [6, 128, 0, 0] },
    Function { key: "返回", bytes: [3, 0, 36, 2] },
    Function { key: "fn锁屏", bytes: [10, 13, 0, 0] },
    Function { key: "锁屏", bytes: [0, 0, 227, 15] },
    Function { key: "r_FN", bytes: [10, 1, 1, 0] },
    Function { key: "音量<->键盘亮度", bytes: [10, 15, 0, 0] },
    Function { key: "音量<->键盘亮度2", bytes: [10, 13, 0, 0] },
    Function { key: "音量<->键盘亮度3", bytes: [10, 14, 0, 0] },
    Function { key: "刷新", bytes: [3, 0, 39, 2] },
    Function { key: "程序打开1", bytes: [6, 128, 0, 1] },
    Function { key: "程序打开2", bytes: [6, 128, 0, 2] },
    Function { key: "程序打开3", bytes: [6, 128, 0, 3] },
    Function { key: "profile_value", bytes: [8, 0, 0, 0] },
    Function { key: "Profile加", bytes: [8, 0, 1, 0] },
    Function { key: "Profile减", bytes: [8, 0, 2, 0] },
    Function { key: "profile_loop", bytes: [8, 0, 3, 0] },
    Function { key: "profile_exchange", bytes: [8, 0, 4, 0] },
    Function { key: "profile_exchange2", bytes: [8, 0, 4, 1] },
    Function { key: "profile_exchange3", bytes: [8, 0, 4, 2] },
    Function { key: "profile_exchange4", bytes: [8, 0, 4, 3] },
    Function { key: "profile_exchange5", bytes: [8, 0, 4, 4] },
    Function { key: "Open website", bytes: [18, 0, 0, 0] },
    Function { key: "KEYCODE_F13", bytes: [0, 104, 0, 0] },
    Function { key: "倒计时", bytes: [10, 24, 0, 0] },
    Function { key: "LEDBIRINC", bytes: [13, 2, 1, 0] },
    Function { key: "LEDBIRDEC", bytes: [13, 2, 2, 0] },
];

pub fn function_by_bytes(b: [u8; 4]) -> Option<&'static Function> {
    FUNCTIONS.iter().find(|f| f.bytes == b)
}

pub fn function_by_key(key: &str) -> Option<&'static Function> {
    FUNCTIONS.iter().find(|f| f.key == key)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyAction {
    Disabled,
    /// Up to three HID usages pressed together, stored as bytes 1..=3 of
    /// `[0, k1, k2, k3]`. A lone key sits in the middle like the factory map.
    Keys([u8; 3]),
    Mouse(MouseButton),
    Function(&'static str),
    Macro { mode: MacroMode, index: u8 },
    /// Hold/tap: `[24, tap, hold, time/10]`.
    ModTap { key: u8, key2: u8, time_ms: u16 },
    Raw([u8; 4]),
}

impl KeyAction {
    /// Builds a key/combo action from pressed HID usages (max 3).
    pub fn keys(hids: &[u8]) -> Self {
        match hids {
            [] => Self::Disabled,
            [k] => Self::Keys([0, *k, 0]),
            [a, b] => Self::Keys([*a, *b, 0]),
            [a, b, c, ..] => Self::Keys([*a, *b, *c]),
        }
    }

    /// The non-zero HID usages of a key/combo action.
    pub fn pressed(&self) -> Vec<u8> {
        match self {
            Self::Keys(k) => k.iter().copied().filter(|&b| b != 0).collect(),
            _ => Vec::new(),
        }
    }

    pub fn encode(&self) -> [u8; 4] {
        match *self {
            Self::Disabled => [0, 0, 0, 0],
            Self::Keys([a, b, c]) => [0, a, b, c],
            Self::Mouse(m) => m.bytes(),
            Self::Function(k) => function_by_key(k).map_or([0; 4], |f| f.bytes),
            Self::Macro { mode, index } => [9, mode.byte(), index, 0],
            Self::ModTap { key, key2, time_ms } => [24, key, key2, (time_ms / 10) as u8],
            Self::Raw(b) => b,
        }
    }

    /// Decodes a main-layer slot (`matrixToConfigs`).
    pub fn decode(l: [u8; 4]) -> Self {
        if l == [0, 0, 0, 0] {
            return Self::Disabled;
        }
        match l[0] {
            9 => Self::Macro { mode: MacroMode::from_byte(l[1]), index: l[2] },
            1 => MouseButton::from_bytes(l).map_or(Self::Raw(l), Self::Mouse),
            _ if function_by_bytes(l).is_some() => Self::Function(function_by_bytes(l).unwrap().key),
            0 => Self::Keys([l[1], l[2], l[3]]),
            24 => Self::ModTap { key: l[1], key2: l[2], time_ms: l[3] as u16 * 10 },
            _ => Self::Raw(l),
        }
    }

    /// Decodes an Fn-layer slot (`changeArrToConfig`): `[0,0,3,0]` also means
    /// "no action" there, and `[0,0,1,0]` is an opaque firmware marker.
    pub fn decode_fn(t: [u8; 4]) -> Self {
        if t == [0, 0, 1, 0] {
            return Self::Raw(t);
        }
        if t == [0, 0, 0, 0] || t == [0, 0, 3, 0] {
            return Self::Disabled;
        }
        Self::decode(t)
    }
}

pub fn slot(matrix: &[u8], i: usize) -> [u8; 4] {
    let mut b = [0u8; 4];
    if let Some(s) = matrix.get(i * 4..i * 4 + 4) {
        b.copy_from_slice(s);
    }
    b
}

pub fn set_slot(matrix: &mut [u8], i: usize, v: [u8; 4]) {
    if let Some(s) = matrix.get_mut(i * 4..i * 4 + 4) {
        s.copy_from_slice(&v);
    }
}

/// The physical key's identity: its factory action (`matrixToHid`).
pub fn slot_hid(i: usize) -> u32 {
    let t = slot(&DEFAULT_MATRIX, i);
    if t[0] == 0 && t[1] == 0 && t[3] == 0 {
        return t[2] as u32;
    }
    ((t[0] as u32) << 24) | ((t[1] as u32) << 16) | ((t[2] as u32) << 8) | t[3] as u32
}

/// Matrix slot of the `nth` key whose factory code is `hid` (`findIndexInDefaultMatrix`).
pub fn slot_of_hid(hid: u32, nth: usize) -> Option<usize> {
    (0..SLOTS).filter(|&i| slot_hid(i) == hid && hid != 0).nth(nth)
}

pub fn default_slot(i: usize) -> [u8; 4] {
    slot(&DEFAULT_MATRIX, i)
}

pub fn default_fn_slot(i: usize) -> [u8; 4] {
    slot(&DEFAULT_FN_MATRIX, i)
}

// ---- macros ------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MacroInput {
    Key(u8),
    Mouse(u8),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MacroEvent {
    /// Key or mouse button transition followed by a delay in ms.
    Press { input: MacroInput, down: bool, delay: u16 },
    Move { dx: i8, dy: i8, delay: u16 },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Macro {
    pub repeat: u16,
    pub events: Vec<MacroEvent>,
}

impl Macro {
    /// Bytes used on the keyboard (`checkUiMacroListIsFull` caps at 248).
    pub fn byte_len(&self) -> usize {
        self.events.iter().map(|e| encode_event(e).len()).sum()
    }

    /// The original editor's limit (`checkUiMacroListIsFull`).
    pub const MAX_EVENT_BYTES: usize = 248;

    pub fn fits(&self) -> bool {
        self.byte_len() <= Self::MAX_EVENT_BYTES
    }

    /// Inputs pressed more often than released (Save refuses these).
    pub fn unreleased(&self) -> Vec<MacroInput> {
        let mut out: Vec<MacroInput> = Vec::new();
        let mut seen: Vec<(MacroInput, i32)> = Vec::new();
        for e in &self.events {
            if let MacroEvent::Press { input, down, .. } = e {
                match seen.iter_mut().find(|(i, _)| i == input) {
                    Some((_, n)) => *n += if *down { 1 } else { -1 },
                    None => seen.push((input.clone(), if *down { 1 } else { -1 })),
                }
            }
        }
        for (i, n) in seen {
            if n > 0 {
                out.push(i);
            }
        }
        out
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![0u8; crate::proto::MACRO_LEN];
        out[0] = self.repeat as u8;
        out[1] = (self.repeat >> 8) as u8;
        let mut at = 2;
        for e in &self.events {
            let b = encode_event(e);
            if at + b.len() > out.len() {
                break;
            }
            out[at..at + b.len()].copy_from_slice(&b);
            at += b.len();
        }
        out
    }

    pub fn decode(t: &[u8]) -> Self {
        if t.len() < 6 {
            return Self::default();
        }
        let repeat = u16::from_le_bytes([t[0], t[1]]);
        let mut events = Vec::new();
        let mut i = 2usize;
        let get4 = |i: usize| -> [u8; 4] {
            let mut s = [0u8; 4];
            for (k, b) in s.iter_mut().enumerate() {
                *b = *t.get(i + k).unwrap_or(&0);
            }
            s
        };
        let mut s = get4(i);
        i += 4;
        while i < t.len() {
            if s[0] == 249 {
                let delay = if s[1] != 0 {
                    (s[1] >> 1) as u16
                } else {
                    let d = u16::from_le_bytes([*t.get(i).unwrap_or(&0), *t.get(i + 1).unwrap_or(&0)]);
                    i += 2;
                    d
                };
                events.push(MacroEvent::Move { dx: s[2] as i8, dy: s[3] as i8, delay });
            } else {
                let down = s[1] > 127;
                let input = if (4..=239).contains(&s[0]) {
                    Some(MacroInput::Key(s[0]))
                } else if MouseButton::from_macro_code(s[0]).is_some() {
                    Some(MacroInput::Mouse(s[0]))
                } else {
                    None
                };
                let delay = if s[1] & 127 != 0 {
                    i -= 2;
                    (s[1] & 127) as u16
                } else {
                    u16::from_le_bytes([s[2], s[3]])
                };
                if let Some(input) = input {
                    events.push(MacroEvent::Press { input, down, delay });
                }
            }
            s = get4(i);
            if s == [0, 0, 0, 0] {
                break;
            }
            i += 4;
        }
        Self { repeat, events }
    }
}

fn encode_event(e: &MacroEvent) -> Vec<u8> {
    let mut r = Vec::with_capacity(6);
    match *e {
        MacroEvent::Press { ref input, down, delay } => {
            r.push(match *input {
                MacroInput::Key(k) | MacroInput::Mouse(k) => k,
            });
            if delay <= 127 {
                r.push(if down { delay as u8 + 128 } else { delay as u8 });
            } else {
                r.push(if down { 128 } else { 0 });
                r.extend_from_slice(&delay.to_le_bytes());
            }
        }
        MacroEvent::Move { dx, dy, delay } => {
            r.push(249);
            r.push(if delay <= 127 { delay as u8 } else { 0 });
            r.push(dx as u8);
            r.push(dy as u8);
            if delay > 127 {
                r.extend_from_slice(&delay.to_le_bytes());
            }
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fn_key_identity() {
        let i = (0..SLOTS).find(|&i| default_slot(i) == [10, 1, 0, 0]).unwrap();
        assert_eq!(slot_hid(i), 0x0A01_0000);
        assert_eq!(slot_of_hid(0x0A01_0000, 0), Some(i));
    }

    #[test]
    fn esc_is_slot_zero() {
        assert_eq!(slot_of_hid(41, 0), Some(0));
    }

    #[test]
    fn action_codec_round_trips() {
        for b in [
            [0, 0, 4, 0],
            [0, 0x02, 4, 0],
            [9, 2, 3, 0],
            [1, 0, 240, 0],
            [3, 0, 233, 0],
            [24, 4, 5, 20],
            [77, 1, 2, 3],
        ] {
            assert_eq!(KeyAction::decode(b).encode(), b, "{b:?}");
        }
    }

    #[test]
    fn macro_round_trip_short_and_long_delays() {
        let m = Macro {
            repeat: 3,
            events: vec![
                MacroEvent::Press { input: MacroInput::Key(4), down: true, delay: 50 },
                MacroEvent::Press { input: MacroInput::Key(4), down: false, delay: 300 },
                MacroEvent::Press { input: MacroInput::Mouse(240), down: true, delay: 20 },
                MacroEvent::Press { input: MacroInput::Mouse(240), down: false, delay: 20 },
            ],
        };
        let bytes = m.encode();
        assert_eq!(&bytes[..2], &[3, 0]);
        assert_eq!(Macro::decode(&bytes), m);
    }
}
