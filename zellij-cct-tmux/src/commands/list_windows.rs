use crate::{format, logger, zellij_bridge};

/// tmux list-windows [-t <session>] [-F <format>]
pub fn run(args: &[&str]) -> i32 {
    let mut _target: Option<&str> = None;
    let mut fmt: Option<&str> = None;
    let mut i = 0;

    while i < args.len() {
        match args[i] {
            "-t" if i + 1 < args.len() => {
                i += 1;
                _target = Some(args[i]);
            }
            "-F" if i + 1 < args.len() => {
                i += 1;
                fmt = Some(args[i]);
            }
            _ => {}
        }
        i += 1;
    }

    let result = zellij_bridge::action(&["list-tabs", "--json"]);
    if result.code != 0 {
        logger::log_msg(&format!(
            "list-windows: zellij list-tabs failed: {}",
            result.stderr.trim()
        ));
        return 1;
    }

    let tabs: Vec<serde_json::Value> = match serde_json::from_str(&result.stdout) {
        Ok(v) => v,
        Err(e) => {
            logger::log_msg(&format!("list-windows: failed to parse JSON: {e}"));
            return 1;
        }
    };

    for tab in &tabs {
        let tab_name = tab
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let tab_position = tab
            .get("position")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        if let Some(template) = fmt {
            let ctx = format::FormatContext {
                window_name: Some(tab_name.to_string()),
                window_index: Some(tab_position),
                ..Default::default()
            };
            println!("{}", format::expand(template, &ctx));
        } else {
            println!("{tab_position}: {tab_name}");
        }
    }

    0
}
