use crate::{logger, tab_resolve, winmap, zellij_bridge};

/// tmux kill-window [-t <target>]
pub fn run(args: &[&str]) -> i32 {
    let mut target: Option<&str> = None;
    let mut i = 0;

    while i < args.len() {
        match args[i] {
            "-t" if i + 1 < args.len() => {
                i += 1;
                target = Some(args[i]);
            }
            _ => {}
        }
        i += 1;
    }

    let Some(t) = target else {
        // No target — close the current tab
        let result = zellij_bridge::action(&["close-tab"]);
        return if result.code == 0 { 0 } else { 1 };
    };

    let Some(resolved) = tab_resolve::resolve(t) else {
        eprintln!("can't find window: {t}");
        return 1;
    };

    // Try headless close via pane-id first to avoid race condition
    // and to work on detached sessions
    if let Some(pane_id) = winmap::WinMap::load().active_pane_for_name(&resolved.name) {
        logger::log_msg(&format!("kill-window: closing tab {} via pane {}", resolved.name, pane_id));
        let result = zellij_bridge::action(&["close-pane", "--pane-id", &pane_id]);
        if result.code == 0 {
            // Mark the window as tombstoned in winmap
            let mut winmap = winmap::WinMap::load();
            winmap.tombstone_name(&resolved.name);
            return 0;
        }
        // Fall through to focus-based approach if pane-id close fails
        logger::log_msg(&format!(
            "kill-window: close-pane for {} failed, trying focus-based close: {}",
            resolved.name,
            result.stderr.trim()
        ));
    }

    // Verify the active tab is our target before closing (race fix)
    let current_tab = tab_resolve::active_tab();
    let is_current_target = current_tab
        .as_ref()
        .map(|tab| tab.name == resolved.name)
        .unwrap_or(false);

    if !is_current_target {
        // Navigate to the target tab first
        let nav = zellij_bridge::action(&["go-to-tab-name", &resolved.name]);
        if nav.code != 0 {
            logger::log_msg(&format!(
                "kill-window: go-to-tab-name failed: {}",
                nav.stderr.trim()
            ));
            return 1;
        }

        // Verify we made it to the target tab (race detection)
        let after_nav = tab_resolve::active_tab();
        let nav_success = after_nav
            .as_ref()
            .map(|tab| tab.name == resolved.name)
            .unwrap_or(false);

        if !nav_success {
            logger::log_msg(&format!(
                "kill-window: race condition detected — active tab after nav was {:?} not {}",
                after_nav, resolved.name
            ));
            eprintln!("can't kill window {t}: tab switched during operation");
            return 1;
        }
    }

    let result = zellij_bridge::action(&["close-tab"]);
    if result.code != 0 {
        logger::log_msg(&format!(
            "kill-window: close-tab failed: {}",
            result.stderr.trim()
        ));
        return 1;
    }

    // Mark the window as tombstoned
    let mut winmap = winmap::WinMap::load();
    winmap.tombstone_name(&resolved.name);

    0
}
