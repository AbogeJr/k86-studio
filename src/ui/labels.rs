//! English captions for key actions.

use crate::keymap::{KeyAction, MacroMode, MouseButton};
use crate::layout;

pub fn function_name(key: &str) -> &str {
    match key {
        "上一曲" => "Previous Track",
        "下一曲" => "Next Track",
        "停止" => "Stop",
        "播放暂停" => "Play/Pause",
        "播放器" => "Media Player",
        "静音" => "Mute",
        "音量减" => "Volume Down",
        "音量加" => "Volume Up",
        "计算器" => "Calculator",
        "fn" => "Fn",
        "r_FN" => "Right Fn",
        "邮件" => "Email",
        "我的电脑" => "My Computer",
        "搜索" => "Search",
        "主页" => "Browser Home",
        "亮度减" => "Brightness Down",
        "亮度加" => "Brightness Up",
        "呼出siri" => "Siri",
        "系统电源" => "Power",
        "系统睡眠" => "Sleep",
        "电脑唤醒" => "Wake",
        "切换输入法" => "Switch Input",
        "缩小" => "Zoom Out",
        "放大" => "Zoom In",
        "麦克风开关" => "Mic Switch",
        "返回" => "Browser Back",
        "fn锁屏" => "Fn Lock",
        "锁屏" => "Lock Screen",
        "音量<->键盘亮度" | "音量<->键盘亮度2" | "音量<->键盘亮度3" => "Volume / Light",
        "刷新" => "Refresh",
        "程序打开1" => "Launch App 1",
        "程序打开2" => "Launch App 2",
        "程序打开3" => "Launch App 3",
        "profile_value" | "profile_loop" => "Profile Cycle",
        "Profile加" => "Profile +",
        "Profile减" => "Profile −",
        "profile_exchange" => "Profile 1",
        "profile_exchange2" => "Profile 2",
        "profile_exchange3" => "Profile 3",
        "profile_exchange4" => "Profile 4",
        "profile_exchange5" => "Profile 5",
        "KEYCODE_F13" => "F13",
        "倒计时" => "Countdown",
        "LEDBIRINC" => "Light +",
        "LEDBIRDEC" => "Light −",
        other => other,
    }
}

pub fn mouse_name(m: MouseButton) -> &'static str {
    match m {
        MouseButton::Left => "Left Click",
        MouseButton::Right => "Right Click",
        MouseButton::Middle => "Wheel Click",
        MouseButton::Forward => "Forward",
        MouseButton::Back => "Back",
        MouseButton::WheelForward => "Wheel Fwd",
        MouseButton::WheelBack => "Wheel Back",
        MouseButton::WheelLeft => "Wheel Left",
        MouseButton::WheelRight => "Wheel Right",
        MouseButton::ScrollUp => "Wheel Up",
        MouseButton::ScrollDown => "Wheel Down",
    }
}

pub fn macro_mode_name(m: MacroMode) -> &'static str {
    match m {
        MacroMode::RepeatTimes => "Play N times",
        MacroMode::WhileHeld => "Repeat while held",
        MacroMode::OnOff => "Toggle on/off",
    }
}

pub fn key_name(hid: u8) -> String {
    layout::key_name(hid as u32).map(str::to_owned).unwrap_or_else(|| format!("0x{hid:02X}"))
}

pub fn action(a: &KeyAction) -> String {
    match a {
        KeyAction::Disabled => "Off".into(),
        KeyAction::Keys(_) => a.pressed().iter().map(|&h| key_name(h)).collect::<Vec<_>>().join(" + "),
        KeyAction::Mouse(m) => mouse_name(*m).into(),
        KeyAction::Function(k) => {
            // Prefer the palette's own wording when the bytes match.
            let b = a.encode();
            layout::FUNCTIONS
                .iter()
                .find(|(_, v)| *v == b)
                .map(|(n, _)| (*n).to_owned())
                .unwrap_or_else(|| function_name(k).to_owned())
        }
        KeyAction::Macro { index, .. } => format!("Macro {}", index + 1),
        KeyAction::ModTap { key, key2, .. } => format!("{} / {}", key_name(*key), key_name(*key2)),
        KeyAction::Raw(b) => format!("{:02X}{:02X}{:02X}{:02X}", b[0], b[1], b[2], b[3]),
    }
}
