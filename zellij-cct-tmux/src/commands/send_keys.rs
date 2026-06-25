use crate::{idmap, keys, logger, ready_wait, tab_resolve, winmap, zellij_bridge};

/// tmux send-keys [-t <target>] [-l] <key>...
pub fn run(args: &[&str]) -> i32 {
    let mut target: Option<&str> = None;
    let mut literal = false;
    let mut key_args: Vec<&str> = Vec::new();
    let mut i = 0;

    while i < args.len() {
        match args[i] {
            "-t" if i + 1 < args.len() => {
                i += 1;
                target = Some(args[i]);
            }
            "-l" => literal = true,
            _ => key_args.push(args[i]),
        }
        i += 1;
    }

    let bytes = if literal {
        key_args.join("").into_bytes()
    } else {
        keys::translate_args(&key_args)
    };

    if bytes.is_empty() {
        return 0;
    }

    let (zellij_pane_id, tmux_id) = if let Some(t) = target {
        let tmux_id = idmap::parse_tmux_pane_id(t);
        match tmux_id {
            Some(id) => {
                let idmap = idmap::IdMap::load();
                match idmap.to_zellij(id) {
                    Some(z) => (z.to_string(), Some(id)),
                    None => {
                        logger::log_msg(&format!("send-keys: unknown pane %{id}"));
                        eprintln!("can't find pane: {t}");
                        return 1;
                    }
                }
            }
            None => {
                // Not a pane ID — window target (@N)? Try winmap lookup first for headless targeting
                if let Some(win_id) = winmap::parse_window_id(t) {
                    if let Some(pane_id) = winmap::WinMap::load().active_pane_for(win_id) {
                        logger::log_msg(&format!(
                            "send-keys: targeting window @{win_id} via pane {} (headless)",
                            pane_id
                        ));
                        return do_send_keys(&pane_id, Some(win_id), &bytes);
                    }
                }

                // Fall back to tab name resolution (requires attached client)
                if let Some(resolved) = tab_resolve::resolve(t) {
                    // Try to get the pane from winmap first for headless operation
                    if let Some(pane_id) =
                        winmap::WinMap::load().active_pane_for_name(&resolved.name)
                    {
                        logger::log_msg(&format!(
                            "send-keys: targeting tab {} via pane {} (headless)",
                            resolved.name, pane_id
                        ));
                        return do_send_keys(&pane_id, None, &bytes);
                    }

                    // Fallback to focus-based approach
                    let nav = zellij_bridge::action(&["go-to-tab-name", &resolved.name]);
                    if nav.code != 0 {
                        logger::log_msg(&format!(
                            "send-keys: failed to switch to tab {}/client detached",
                            resolved.name
                        ));
                        eprintln!("can't send keys to {t}: session not attached");
                        return 1;
                    }
                    (String::new(), None)
                } else {
                    eprintln!("bad pane id: {t}");
                    return 1;
                }
            }
        }
    } else {
        (String::new(), None)
    };

    do_send_keys(&zellij_pane_id, tmux_id, &bytes)
}

fn do_send_keys(zellij_pane_id: &str, tmux_id: Option<u32>, bytes: &[u8]) -> i32 {
    // Race fix: if this pane was created recently, wait for shell prompt
    if let Some(id) = tmux_id {
        let idmap = idmap::IdMap::load();
        if let Some(created_at) = idmap.created_at(id) {
            if ready_wait::is_recently_created(created_at) {
                logger::log_msg(&format!(
                    "send-keys: pane %{id} created recently, waiting for prompt"
                ));
                let detected = ready_wait::wait_for_prompt(zellij_pane_id);
                if !detected {
                    logger::log_msg(&format!(
                        "send-keys: prompt not detected for %{id}, sending anyway"
                    ));
                }
            }
        }
    }

    let bytes_str = String::from_utf8_lossy(bytes).to_string();

    let action_args = if !zellij_pane_id.is_empty() {
        vec!["write-chars", "--pane-id", zellij_pane_id, &bytes_str]
    } else {
        vec!["write-chars", &bytes_str]
    };

    let result = zellij_bridge::action(&action_args);
    if result.code != 0 {
        logger::log_msg(&format!(
            "send-keys: zellij write-chars failed: {}",
            result.stderr.trim()
        ));
        return 1;
    }

    0
}
