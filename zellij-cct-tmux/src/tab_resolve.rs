use crate::{logger, winmap, zellij_bridge};

pub struct ResolvedTab {
    pub position: u32,
    pub name: String,
}

/// Strip the tmux `session:` prefix and `.pane` suffix from a target,
/// leaving just the window reference (`@N`, an index, or a tab name).
///
/// tmux targets are `[session:][window][.pane]`. Zellij tab names never
/// contain `:` or `.`, so splitting on the first of each is safe here.
pub fn window_ref(target: &str) -> &str {
    let after_session = target.split_once(':').map(|(_, rest)| rest).unwrap_or(target);
    after_session
        .split_once('.')
        .map(|(win, _)| win)
        .unwrap_or(after_session)
}

/// Resolve a tmux window target to a zellij tab. Accepts a `@N` window id, a
/// numeric index, a bare tab name, or any of those behind a `session:` prefix.
pub fn resolve(target: &str) -> Option<ResolvedTab> {
    let target = window_ref(target);

    // `@N` window id → tab name (via the window id map) → tab.
    if let Some(id) = winmap::parse_window_id(target) {
        let name = winmap::WinMap::load().name_for(id)?;
        return query_tabs()?.into_iter().find(|t| t.name == name);
    }

    let tabs = query_tabs()?;

    if let Ok(index) = target.parse::<u32>() {
        return tabs.into_iter().find(|t| t.position == index);
    }

    tabs.into_iter().find(|t| t.name == target)
}

/// The currently-focused zellij tab. Used to discover the tab a `new-window`
/// just created when no explicit name was supplied.
pub fn active_tab() -> Option<ResolvedTab> {
    let result = zellij_bridge::action(&["list-tabs", "--json"]);
    if result.code != 0 {
        return None;
    }
    let json = serde_json::from_str::<Vec<serde_json::Value>>(&result.stdout).ok()?;
    json.into_iter().find_map(|tab| {
        if !tab.get("active")?.as_bool()? {
            return None;
        }
        let name = tab.get("name")?.as_str()?.to_string();
        let position = tab.get("position")?.as_u64()? as u32;
        Some(ResolvedTab { position, name })
    })
}

pub fn query_tabs() -> Option<Vec<ResolvedTab>> {
    let result = zellij_bridge::action(&["list-tabs", "--json"]);
    if result.code == 0 {
        if let Ok(json) = serde_json::from_str::<Vec<serde_json::Value>>(&result.stdout) {
            return Some(
                json.iter()
                    .filter_map(|tab| {
                        let name = tab.get("name")?.as_str()?.to_string();
                        let position = tab.get("position")?.as_u64()? as u32;
                        Some(ResolvedTab { position, name })
                    })
                    .collect(),
            );
        }
    }

    let result = zellij_bridge::action(&["query-tab-names"]);
    if result.code != 0 || result.stderr.contains("not found") {
        logger::log_msg("tab_resolve: both list-tabs and query-tab-names failed");
        return None;
    }

    Some(
        result
            .stdout
            .lines()
            .enumerate()
            .map(|(i, name)| ResolvedTab {
                position: i as u32,
                name: name.trim().to_string(),
            })
            .filter(|t| !t.name.is_empty())
            .collect(),
    )
}
