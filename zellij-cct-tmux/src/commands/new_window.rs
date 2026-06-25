use crate::{format, logger, tab_resolve, winmap, zellij_bridge};

/// tmux new-window [-d] [-t <session>] [-n <name>] [-c <dir>] [-P] [-F <format>]
pub fn run(args: &[&str]) -> i32 {
    let mut _target: Option<&str> = None;
    let mut window_name: Option<String> = None;
    let mut cwd: Option<String> = None;
    let mut print_info = false;
    let mut format_str: Option<&str> = None;
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
            _ => {}
        }
        i += 1;
    }

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
