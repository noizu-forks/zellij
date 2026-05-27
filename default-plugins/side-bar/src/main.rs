//! Side-bar plugin for zellij side-tabs.
//!
//! Renders a vertical column of emoji icons representing the side tabs
//! attached to the currently active main tab. Clicking a row switches
//! to that side tab; the bottom "+" row is reserved for future
//! "new side tab" action.
//!
//! Layout sketch (collapsed, cols <= 3):
//!
//!   ┌──┐
//!   │🔧│  <- active (highlighted)
//!   │📊│
//!   │📝│
//!   │+ │
//!   └──┘
//!
//! Expanded (cols > 3 or InputMode::Tab):
//!
//!   ┌──────────┐
//!   │🔧 build  │  <- active
//!   │📊 stats  │
//!   │📝 notes  │
//!   │+  new    │
//!   └──────────┘

use std::collections::BTreeMap;
use zellij_tile::prelude::*;
// zellij_tile_utils provides the style! macro — we use manual ANSI
// escapes instead for finer control over the vertical layout, but keep
// the dep available for future use.

// ── State ────────────────────────────────────────────────────────────

#[derive(Default)]
struct State {
    /// All tabs reported by the host, including both main and side tabs.
    tabs: Vec<TabInfo>,
    /// Cached mode info for styling and mode detection.
    mode_info: ModeInfo,
    /// Side tabs filtered for the active main tab (rebuilt each update).
    side_tabs: Vec<TabInfo>,
    /// Whether the sidebar is in expanded view (shows emoji + name).
    is_expanded: bool,
}

register_plugin!(State);

// ── Helpers ──────────────────────────────────────────────────────────

/// Find the currently-active main tab (a tab with no parent_tab_id).
fn active_main_tab(tabs: &[TabInfo]) -> Option<&TabInfo> {
    tabs.iter().find(|t| t.active && t.parent_tab_id.is_none())
}

/// Find the active main tab's ID. If the user has focused a *side* tab,
/// walk up to its parent so we still know which group to display.
fn active_main_tab_id(tabs: &[TabInfo]) -> Option<usize> {
    // First: check if a main tab is active.
    if let Some(main) = active_main_tab(tabs) {
        return Some(main.tab_id);
    }
    // Fallback: the active tab is itself a side tab — use its parent.
    tabs.iter()
        .find(|t| t.active)
        .and_then(|t| t.parent_tab_id)
}

/// Collect side tabs belonging to `main_id`, sorted by position.
fn side_tabs_for(tabs: &[TabInfo], main_id: usize) -> Vec<TabInfo> {
    let mut side: Vec<TabInfo> = tabs
        .iter()
        .filter(|t| t.parent_tab_id == Some(main_id))
        .cloned()
        .collect();
    side.sort_by_key(|t| t.position);
    side
}

/// ANSI escape: set foreground from a PaletteColor.
fn fg_color(c: &PaletteColor) -> String {
    match c {
        PaletteColor::Rgb((r, g, b)) => format!("\u{1b}[38;2;{};{};{}m", r, g, b),
        PaletteColor::EightBit(n) => format!("\u{1b}[38;5;{}m", n),
    }
}

/// ANSI escape: set background from a PaletteColor.
fn bg_color(c: &PaletteColor) -> String {
    match c {
        PaletteColor::Rgb((r, g, b)) => format!("\u{1b}[48;2;{};{};{}m", r, g, b),
        PaletteColor::EightBit(n) => format!("\u{1b}[48;5;{}m", n),
    }
}

const RESET: &str = "\u{1b}[0m";

// ── Plugin impl ──────────────────────────────────────────────────────

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        set_selectable(false);
        subscribe(&[
            EventType::TabUpdate,
            EventType::ModeUpdate,
            EventType::Mouse,
        ]);
    }

    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;

        match event {
            Event::ModeUpdate(mode_info) => {
                let was_expanded = self.is_expanded;
                self.is_expanded = mode_info.mode == InputMode::Tab;
                if self.mode_info != mode_info || was_expanded != self.is_expanded {
                    should_render = true;
                }
                self.mode_info = mode_info;
            },
            Event::TabUpdate(tabs) => {
                let main_id = active_main_tab_id(&tabs);
                let new_side = main_id
                    .map(|id| side_tabs_for(&tabs, id))
                    .unwrap_or_default();

                if self.tabs != tabs || self.side_tabs != new_side {
                    should_render = true;
                }
                self.tabs = tabs;
                self.side_tabs = new_side;
            },
            Event::Mouse(me) => match me {
                Mouse::LeftClick(row, _col) => {
                    let row = row as usize;
                    if row < self.side_tabs.len() {
                        // position is 0-based; switch_tab_to wants 1-based
                        let pos = self.side_tabs[row].position;
                        switch_tab_to((pos + 1) as u32);
                    }
                    // If they clicked the "+" row we could trigger a new-side-tab
                    // action here in the future.
                },
                _ => {},
            },
            _ => {},
        }

        should_render
    }

    fn render(&mut self, rows: usize, cols: usize) {
        if self.side_tabs.is_empty() {
            // Nothing to show — fill with background and bail.
            let bg = &self.mode_info.style.colors.text_unselected.background;
            for _ in 0..rows {
                print!("{}{}\u{1b}[0K\n", bg_color(bg), RESET);
            }
            return;
        }

        let colors = &self.mode_info.style.colors;
        let bg_unsel = &colors.text_unselected.background;
        let fg_unsel = &colors.text_unselected.base;
        let bg_sel = &colors.ribbon_selected.background;
        let fg_sel = &colors.ribbon_selected.base;

        let expanded = self.is_expanded || cols > 3;

        for (i, tab) in self.side_tabs.iter().enumerate() {
            if i >= rows.saturating_sub(1) {
                // Reserve last row for "+"
                break;
            }

            let emoji = tab
                .side_tab_emoji
                .as_deref()
                .unwrap_or("?");

            let is_active = tab.active;

            let (fg, bg) = if is_active {
                (fg_color(fg_sel), bg_color(bg_sel))
            } else {
                (fg_color(fg_unsel), bg_color(bg_unsel))
            };

            if expanded {
                // emoji + space + name, truncated to fit
                let label = &tab.name;
                // Emoji typically occupies 2 columns; name fills the rest
                let max_name = cols.saturating_sub(3); // 2 for emoji + 1 space
                let name: String = label.chars().take(max_name).collect();
                print!("{}{}{} {}\u{1b}[0K{}\n", fg, bg, emoji, name, RESET);
            } else {
                // Collapsed: just the emoji, centered-ish
                print!("{}{}{}\u{1b}[0K{}\n", fg, bg, emoji, RESET);
            }
        }

        // "+" row for creating new side tabs
        let rendered_count = std::cmp::min(self.side_tabs.len(), rows.saturating_sub(1));
        if rendered_count < rows {
            let fg = fg_color(fg_unsel);
            let bg = bg_color(bg_unsel);
            if expanded {
                print!("{}{}+  new\u{1b}[0K{}\n", fg, bg, RESET);
            } else {
                print!("{}{}+\u{1b}[0K{}\n", fg, bg, RESET);
            }
            // Fill remaining rows with background
            for _ in (rendered_count + 1)..rows {
                print!("{}\u{1b}[0K{}\n", bg_color(bg_unsel), RESET);
            }
        }
    }
}
