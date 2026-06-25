use crate::{format, tab_resolve, zellij_bridge};

struct SessionInfo {
    name: String,
    current: bool,
}

/// tmux list-sessions [-F <format>]
///
/// Zellij's `list-sessions` output is ANSI-decorated and includes EXITED
/// (resurrectable) sessions that tmux would never report. We translate it to
/// tmux-shaped lines so callers can parse `name:` and `#{session_name}`.
pub fn run(args: &[&str]) -> i32 {
    let mut fmt: Option<&str> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "-F" if i + 1 < args.len() => {
                i += 1;
                fmt = Some(args[i]);
            }
            _ => {}
        }
        i += 1;
    }

    let sessions = match list_active_sessions() {
        Some(s) => s,
        None => return 1,
    };

    // Only the current (attached) session has tabs we can enumerate.
    let current_windows = tab_resolve::query_tabs().map(|t| t.len()).unwrap_or(1).max(1);

    for s in &sessions {
        if let Some(template) = fmt {
            let ctx = format::FormatContext {
                session_name: Some(s.name.clone()),
                window_index: Some(if s.current { current_windows as u32 } else { 1 }),
                ..Default::default()
            };
            println!("{}", format::expand(template, &ctx));
        } else {
            let windows = if s.current { current_windows } else { 1 };
            let plural = if windows == 1 { "window" } else { "windows" };
            let attached = if s.current { " (attached)" } else { "" };
            println!("{}: {} {}{}", s.name, windows, plural, attached);
        }
    }

    0
}

fn list_active_sessions() -> Option<Vec<SessionInfo>> {
    let result = zellij_bridge::command(&["list-sessions"]);
    if result.code != 0 {
        return None;
    }

    let mut sessions = Vec::new();
    for line in result.stdout.lines() {
        let clean = strip_ansi(line);
        let clean = clean.trim();
        if clean.is_empty() || clean.contains("EXITED") {
            continue;
        }
        let name = match clean.split_whitespace().next() {
            Some(n) if !n.is_empty() => n.to_string(),
            _ => continue,
        };
        let current = clean.contains("(current)");
        sessions.push(SessionInfo { name, current });
    }
    Some(sessions)
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_escape = false;
    for ch in s.chars() {
        if in_escape {
            if ch.is_ascii_alphabetic() {
                in_escape = false;
            }
        } else if ch == '\x1b' {
            in_escape = true;
        } else {
            out.push(ch);
        }
    }
    out
}
