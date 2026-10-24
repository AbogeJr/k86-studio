//! Local app data (macro library, preferences) in `~/.config/k86-studio`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::keymap::Macro;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct NamedMacro {
    pub name: String,
    pub data: Macro,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Store {
    pub macros: Vec<NamedMacro>,
    pub screen_delay: u8,
    /// "Minimize to tray when closed".
    pub close_to_tray: Option<bool>,
}

fn dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("k86-studio"))
}

impl Store {
    pub fn load() -> Self {
        let mut s: Self = dir()
            .and_then(|d| std::fs::read(d.join("store.json")).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if s.screen_delay == 0 {
            s.screen_delay = 50;
        }
        s
    }

    pub fn save(&self) {
        let Some(d) = dir() else { return };
        let _ = std::fs::create_dir_all(&d);
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let tmp = d.join("store.json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(tmp, d.join("store.json"));
            }
        }
    }
}
