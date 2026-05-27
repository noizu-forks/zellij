use std::collections::BTreeMap;
use zellij_tile::prelude::*;

#[derive(Default, Debug, Clone, PartialEq)]
struct SideTabEntry {
    emoji: String,
    name: String,
    position: usize,
    active: bool,
}

#[derive(Default)]
struct State {
    side_tabs: Vec<SideTabEntry>,
    mode_info: ModeInfo,
    is_expanded: bool,
}

register_plugin!(State);

fn fg_color(c: &PaletteColor) -> String {
    match c {
        PaletteColor::Rgb((r, g, b)) => format!("\u{1b}[38;2;{};{};{}m", r, g, b),
        PaletteColor::EightBit(n) => format!("\u{1b}[38;5;{}m", n),
    }
}

fn bg_color(c: &PaletteColor) -> String {
    match c {
        PaletteColor::Rgb((r, g, b)) => format!("\u{1b}[48;2;{};{};{}m", r, g, b),
        PaletteColor::EightBit(n) => format!("\u{1b}[48;5;{}m", n),
    }
}

const RESET: &str = "\u{1b}[0m";

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        set_selectable(false);
        subscribe(&[EventType::TabUpdate, EventType::ModeUpdate, EventType::Mouse]);
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
                // Find the active main tab ID (walk up from side tab if needed)
                let main_id = tabs.iter()
                    .find(|t| t.active && t.parent_tab_id.is_none())
                    .map(|t| t.tab_id)
                    .or_else(|| tabs.iter().find(|t| t.active).and_then(|t| t.parent_tab_id));

                // Extract side tab data without cloning TabInfo
                let mut new_side: Vec<SideTabEntry> = Vec::new();
                if let Some(mid) = main_id {
                    for t in tabs.iter() {
                        if t.parent_tab_id == Some(mid) {
                            new_side.push(SideTabEntry {
                                emoji: t.side_tab_emoji.as_deref().unwrap_or("?").to_string(),
                                name: t.name.clone(),
                                position: t.position,
                                active: t.active,
                            });
                        }
                    }
                    new_side.sort_by_key(|e| e.position);
                }

                if self.side_tabs != new_side {
                    should_render = true;
                }
                self.side_tabs = new_side;
            },
            Event::Mouse(me) => {
                if let Mouse::LeftClick(row, _col) = me {
                    let row = (row as usize).saturating_sub(1);
                    if row < self.side_tabs.len() {
                        let pos = self.side_tabs[row].position;
                        switch_tab_to((pos + 1) as u32);
                    } else if row == self.side_tabs.len() {
                        run_action(
                            actions::Action::NewSideTab { layout: None, name: None },
                            BTreeMap::new(),
                        );
                    }
                }
            },
            _ => {},
        }

        should_render
    }

    fn render(&mut self, rows: usize, cols: usize) {
        let colors = &self.mode_info.style.colors;
        let bg_unsel = &colors.text_unselected.background;
        let fg_unsel = &colors.text_unselected.base;
        let bg_sel = &colors.ribbon_selected.background;
        let fg_sel = &colors.ribbon_selected.base;

        // Skip first row to avoid being occluded by the tab-bar above
        print!("{}\u{1b}[0K{}\n", bg_color(bg_unsel), RESET);
        let rows = rows.saturating_sub(1);

        if self.side_tabs.is_empty() {
            for _ in 0..rows {
                print!("{}\u{1b}[0K{}\n", bg_color(bg_unsel), RESET);
            }
            return;
        }

        let expanded = self.is_expanded || cols > 3;

        for (i, entry) in self.side_tabs.iter().enumerate() {
            if i >= rows.saturating_sub(1) {
                break;
            }

            let (fg, bg) = if entry.active {
                (fg_color(fg_sel), bg_color(bg_sel))
            } else {
                (fg_color(fg_unsel), bg_color(bg_unsel))
            };

            if expanded {
                let max_name = cols.saturating_sub(3);
                let name: String = entry.name.chars().take(max_name).collect();
                print!("{}{}{} {}\u{1b}[0K{}\n", fg, bg, entry.emoji, name, RESET);
            } else {
                print!("{}{}{}\u{1b}[0K{}\n", fg, bg, entry.emoji, RESET);
            }
        }

        let rendered = std::cmp::min(self.side_tabs.len(), rows.saturating_sub(1));
        if rendered < rows {
            let fg = fg_color(fg_unsel);
            let bg = bg_color(bg_unsel);
            if expanded {
                print!("{}{}+  new\u{1b}[0K{}\n", fg, bg, RESET);
            } else {
                print!("{}{}+\u{1b}[0K{}\n", fg, bg, RESET);
            }
            for _ in (rendered + 1)..rows {
                print!("{}\u{1b}[0K{}\n", bg_color(bg_unsel), RESET);
            }
        }
    }
}
