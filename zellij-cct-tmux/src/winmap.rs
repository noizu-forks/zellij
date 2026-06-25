use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

/// Persistent map of tmux window ids (`@N`) to zellij tab names.
///
/// tmux addresses windows by an opaque id (`@N`) that survives renames and
/// index shuffles. Zellij has no such handle — tabs are addressed by name or
/// live position — so we mint our own stable ids here and translate back to a
/// tab name when a window target needs to be resolved.
///
/// We also track the active pane_id for each tab to enable headless targeting
/// of send-keys and capture-pane to specific windows/tabs even when detached.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WinEntry {
    tmux_id: u32,
    tab_name: String,
    #[serde(default)]
    tombstoned: bool,
    /// The zellij terminal_N id of the first/active pane in this tab
    #[serde(default)]
    active_pane_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct WinMapData {
    next_id: u32,
    entries: Vec<WinEntry>,
}

pub struct WinMap {
    data: WinMapData,
    path: PathBuf,
}

impl WinMap {
    pub fn load() -> Self {
        let path = state_path();
        let data = fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        WinMap { data, path }
    }

    fn save(&self) {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.data) {
            let _ = fs::write(&self.path, json);
        }
    }

    /// Return the existing id for a tab name, allocating a fresh one if needed.
    /// Idempotent: the same name always maps to the same id for the session.
    pub fn id_for(&mut self, tab_name: &str) -> u32 {
        if let Some(entry) = self
            .data
            .entries
            .iter()
            .find(|e| e.tab_name == tab_name && !e.tombstoned)
        {
            return entry.tmux_id;
        }
        let tmux_id = self.data.next_id;
        self.data.next_id += 1;
        self.data.entries.push(WinEntry {
            tmux_id,
            tab_name: tab_name.to_string(),
            tombstoned: false,
            active_pane_id: None,
        });
        self.save();
        tmux_id
    }

    /// Resolve a window id (`@N` numeric part) back to its tab name.
    pub fn name_for(&self, tmux_id: u32) -> Option<String> {
        self.data
            .entries
            .iter()
            .find(|e| e.tmux_id == tmux_id && !e.tombstoned)
            .map(|e| e.tab_name.clone())
    }

    /// Re-point an id's tab name after a rename, preserving the id.
    pub fn rename(&mut self, old_name: &str, new_name: &str) {
        if let Some(entry) = self
            .data
            .entries
            .iter_mut()
            .find(|e| e.tab_name == old_name && !e.tombstoned)
        {
            entry.tab_name = new_name.to_string();
            self.save();
        }
    }

    /// Get the active pane_id for a window by its tmux id.
    pub fn active_pane_for(&self, tmux_id: u32) -> Option<String> {
        self.data
            .entries
            .iter()
            .find(|e| e.tmux_id == tmux_id && !e.tombstoned)
            .and_then(|e| e.active_pane_id.clone())
    }

    /// Get the active pane_id for a window by its tab name.
    pub fn active_pane_for_name(&self, tab_name: &str) -> Option<String> {
        self.data
            .entries
            .iter()
            .find(|e| e.tab_name == tab_name && !e.tombstoned)
            .and_then(|e| e.active_pane_id.clone())
    }

    /// Set the active pane_id for a window by its tmux id.
    pub fn set_active_pane(&mut self, tmux_id: u32, pane_id: &str) {
        if let Some(entry) = self
            .data
            .entries
            .iter_mut()
            .find(|e| e.tmux_id == tmux_id && !e.tombstoned)
        {
            entry.active_pane_id = Some(pane_id.to_string());
            self.save();
        }
    }

    pub fn tombstone_name(&mut self, tab_name: &str) {
        if let Some(entry) = self
            .data
            .entries
            .iter_mut()
            .find(|e| e.tab_name == tab_name && !e.tombstoned)
        {
            entry.tombstoned = true;
            self.save();
        }
    }
}

/// Parse a tmux window target ("@N") to its numeric id.
pub fn parse_window_id(target: &str) -> Option<u32> {
    target.strip_prefix('@').and_then(|s| s.parse().ok())
}

fn state_path() -> PathBuf {
    let session = env::var("ZELLIJ_SESSION_NAME").unwrap_or_else(|_| "unknown".into());
    let base = env::var("XDG_RUNTIME_DIR")
        .or_else(|_| env::var("TMPDIR"))
        .unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(base)
        .join("zellij-cct")
        .join(&session)
        .join("winmap.json")
}
