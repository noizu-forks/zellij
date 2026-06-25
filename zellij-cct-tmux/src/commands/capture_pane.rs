use crate::{idmap, logger, tab_resolve, winmap, zellij_bridge};

/// tmux capture-pane [-p] [-e] [-t <target>] [-S <start>] [-E <end>]
///
/// Dumps a pane's on-screen contents. Zellij's `dump-screen` can only target the
/// *focused* pane, so for a window/tab target we briefly switch focus to it, dump,
/// then restore the previously-focused tab — the user's view ends up where it
/// started rather than being left on the captured tab.
///
/// When we have a pane-id (from winmap), we can do headless captures without
/// disturbing focus using `dump-screen -p terminal_N`.
pub fn run(args: &[&str]) -> i32 {
    let mut print_to_stdout = false;
    let mut target: Option<&str> = None;
    let mut ansi = false;
    let mut full = false;
    let mut i = 0;

    while i < args.len() {
        match args[i] {
            "-p" => print_to_stdout = true,
            "-e" => ansi = true,
            "-t" if i + 1 < args.len() => {
                i += 1;
                target = Some(args[i]);
            }
            // Any explicit start line means the caller wants scrollback, which
            // zellij exposes via `--full`. We don't honor the exact line count.
            "-S" if i + 1 < args.len() => {
                i += 1;
                full = true;
            }
            // Accept-and-ignore the matching end-line flag.
            "-E" if i + 1 < args.len() => {
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }

    // Try pane-id targeting first for headless operation
    if let Some(t) = target {
        // Check if it's a direct pane ID (%N)
        if let Some(pane_tmux_id) = idmap::parse_tmux_pane_id(t) {
            if let Some(pane_id) = idmap::IdMap::load().to_zellij(pane_tmux_id) {
                return do_capture(pane_id, print_to_stdout, ansi, full, None);
            }
        }

        // Check if it's a window ID (@N)
        if let Some(win_id) = winmap::parse_window_id(t) {
            if let Some(pane_id) = winmap::WinMap::load().active_pane_for(win_id) {
                logger::log_msg(&format!(
                    "capture-pane: capturing window @{win_id} via pane {} (headless)",
                    pane_id
                ));
                return do_capture(&pane_id, print_to_stdout, ansi, full, None);
            }
        }

        // Resolve as tab name
        if let Some(resolved) = tab_resolve::resolve(t) {
            // Try to get the pane from winmap for headless capture
            if let Some(pane_id) = winmap::WinMap::load().active_pane_for_name(&resolved.name) {
                logger::log_msg(&format!(
                    "capture-pane: capturing tab {} via pane {} (headless)",
                    resolved.name, pane_id
                ));
                return do_capture(&pane_id, print_to_stdout, ansi, full, None);
            }

            // Fallback to focus-based approach (requires attached client)
            logger::log_msg(&format!(
                "capture-pane: no pane-id for {}, falling back to focus-based capture",
                resolved.name
            ));
            let previous = tab_resolve::active_tab();
            let nav = zellij_bridge::action(&["go-to-tab-name", &resolved.name]);
            if nav.code != 0 {
                logger::log_msg(&format!(
                    "capture-pane: failed to switch to tab {}/session detached",
                    resolved.name
                ));
                eprintln!("can't capture pane {t}: session not attached");
                return 1;
            }

            let result = do_capture("", print_to_stdout, ansi, full, None);

            // Restore the user's original tab regardless of how the dump went.
            if let Some(prev) = previous.filter(|p| p.name != resolved.name) {
                let nav = if prev.name.trim().is_empty() {
                    zellij_bridge::action(&["go-to-tab", &(prev.position + 1).to_string()])
                } else {
                    zellij_bridge::action(&["go-to-tab-name", &prev.name])
                };
                if nav.code != 0 {
                    logger::log_msg("capture-pane: failed to restore previous tab focus");
                }
            }

            return result;
        } else {
            eprintln!("can't find window: {t}");
            return 1;
        }
    }

    // No target - capture focused pane
    do_capture("", print_to_stdout, ansi, full, None)
}

fn do_capture(
    pane_id: &str,
    print_to_stdout: bool,
    ansi: bool,
    full: bool,
    _restore: Option<tab_resolve::ResolvedTab>,
) -> i32 {
    let mut action_args = vec!["dump-screen"];
    if full {
        action_args.push("--full");
    }
    if ansi {
        action_args.push("--ansi");
    }

    if !pane_id.is_empty() {
        action_args.push("--pane-id");
        action_args.push(pane_id);
    }

    let result = zellij_bridge::action(&action_args);

    if result.code != 0 {
        logger::log_msg(&format!(
            "capture-pane: dump-screen failed: {}",
            result.stderr.trim()
        ));
        return 1;
    }

    if print_to_stdout {
        print!("{}", result.stdout);
    }

    0
}
