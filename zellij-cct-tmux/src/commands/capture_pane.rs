use crate::{logger, tab_resolve, zellij_bridge};

/// tmux capture-pane [-p] [-e] [-t <target>] [-S <start>] [-E <end>]
///
/// Dumps a pane's on-screen contents. Zellij's `dump-screen` can only target the
/// *focused* pane, so for a window/tab target we briefly switch focus to it, dump,
/// then restore the previously-focused tab — the user's view ends up where it
/// started rather than being left on the captured tab.
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

    // Resolve the target window/tab and switch focus to it, remembering where we
    // were so we can switch back afterwards.
    let restore = match target {
        Some(t) => {
            let resolved = match tab_resolve::resolve(t) {
                Some(r) => r,
                None => {
                    eprintln!("can't find window: {t}");
                    return 1;
                }
            };
            let previous = tab_resolve::active_tab();
            let nav = zellij_bridge::action(&["go-to-tab-name", &resolved.name]);
            if nav.code != 0 {
                logger::log_msg(&format!(
                    "capture-pane: failed to switch to tab {}",
                    resolved.name
                ));
                return 1;
            }
            // Only restore if we actually moved off a different tab.
            previous.filter(|p| p.name != resolved.name)
        }
        None => None,
    };

    let mut action_args = vec!["dump-screen"];
    if full {
        action_args.push("--full");
    }
    if ansi {
        action_args.push("--ansi");
    }
    let result = zellij_bridge::action(&action_args);

    // Restore the user's original tab regardless of how the dump went. Unnamed
    // tabs (name == "") can't be reached by name, so fall back to 1-based index.
    if let Some(prev) = restore {
        let nav = if prev.name.trim().is_empty() {
            zellij_bridge::action(&["go-to-tab", &(prev.position + 1).to_string()])
        } else {
            zellij_bridge::action(&["go-to-tab-name", &prev.name])
        };
        if nav.code != 0 {
            logger::log_msg("capture-pane: failed to restore previous tab focus");
        }
    }

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
