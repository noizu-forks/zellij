use crate::{format, logger, tab_resolve, winmap, zellij_bridge};

/// tmux new-window [-d] [-t <session>] [-n <name>] [-c <dir>] [-P] [-F <format>]
pub fn run(args: &[&str]) -> i32 {
    let mut _target: Option<&str> = None;
    let mut window_name: Option<String> = None;
    let mut cwd: Option<String> = None;
    let mut print_info = false;
    let mut format_str: Option<&str> = None;
    let mut detached = false;
    let mut i = 0;

    while i < args.len() {
        match args[i] {
            "-t" if i + 1 < args.len() => {
                i += 1;
                _target = Some(args[i]);
            }
            "-n" if i + 1 < args.len() => {
                i += 1;
                window_name = Some(args[i].to_string());
            }
            "-c" if i + 1 < args.len() => {
                i += 1;
                cwd = Some(args[i].to_string());
            }
            "-P" => print_info = true,
            "-F" if i + 1 < args.len() => {
                i += 1;
                format_str = Some(args[i]);
            }
            "-d" => detached = true,
            _ => {}
        }
        i += 1;
    }

    // For -d, remember the currently active tab to restore later
    let previously_active = tab_resolve::active_tab();

    let mut action_args = vec!["new-tab"];
    if let Some(ref name) = window_name {
        action_args.extend_from_slice(&["--name", name]);
    }
    if let Some(ref dir) = cwd {
        action_args.extend_from_slice(&["--cwd", dir]);
    }

    let result = zellij_bridge::action(&action_args);
    if result.code != 0 {
        logger::log_msg(&format!(
            "new-window: zellij new-tab failed: {}",
            result.stderr.trim()
        ));
        return 1;
    }

    // Determine the tab we just created. With -n we know the name; otherwise the
    // new tab is the active one — and if zellij left it unnamed we give it a
    // generated name so it remains addressable by name/id later.
    let (tab_name, position) = match window_name {
        Some(name) => {
            let pos = tab_resolve::query_tabs()
                .and_then(|tabs| tabs.into_iter().find(|t| t.name == name).map(|t| t.position));
            (name, pos)
        }
        None => match tab_resolve::active_tab() {
            Some(tab) if !tab.name.trim().is_empty() => (tab.name, Some(tab.position)),
            Some(tab) => {
                let generated = format!("cct-win-{}", tab.position);
                let _ = zellij_bridge::action(&["rename-tab", &generated]);
                (generated, Some(tab.position))
            }
            None => {
                logger::log_msg("new-window: could not resolve created tab");
                (String::new(), None)
            }
        },
    };

    // Capture the active pane ID for this tab and store it in winmap
    if !tab_name.is_empty() {
        if let Some(pane_id) = zellij_bridge::active_pane_for_tab(&tab_name) {
            let mut winmap = winmap::WinMap::load();
            let win_id = winmap.id_for(&tab_name);
            winmap.set_active_pane(win_id, &pane_id);
            logger::log_msg(&format!(
                "new-window: mapped window @{win_id} ({}) to pane {}",
                tab_name, pane_id
            ));
        } else {
            logger::log_msg(&format!(
                "new-window: could not find active pane for tab {}",
                tab_name
            ));
        }
    }

    // Restore previous tab if -d was specified
    if detached {
        if let Some(prev_tab) = previously_active {
            if prev_tab.name != tab_name {
                let nav = zellij_bridge::action(&["go-to-tab-name", &prev_tab.name]);
                if nav.code != 0 {
                    logger::log_msg(&format!(
                        "new-window: failed to restore previous tab {}: {}",
                        prev_tab.name,
                        nav.stderr.trim()
                    ));
                }
            }
        }
    }

    let mut winmap = winmap::WinMap::load();
    let win_id = winmap.id_for(&tab_name);

    if print_info {
        let ctx = format::FormatContext {
            window_id: Some(format!("@{win_id}")),
            window_name: Some(tab_name.clone()),
            window_index: position,
            ..Default::default()
        };
        let out = match format_str {
            Some(fmt) => format::expand(fmt, &ctx),
            None => format!("@{win_id}"),
        };
        println!("{out}");
    }

    0
}
