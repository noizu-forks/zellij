use crate::format::{self, FormatContext};

/// tmux display-message [-t <target>] [-p] [<format>]
pub fn run(args: &[&str]) -> i32 {
    let mut print_to_stdout = false;
    let mut target: Option<&str> = None;
    let mut template: Option<&str> = None;
    let mut i = 0;

    while i < args.len() {
        match args[i] {
            "-p" => print_to_stdout = true,
            "-t" if i + 1 < args.len() => {
                i += 1;
                target = Some(args[i]);
            }
            _ => {
                if template.is_none() {
                    template = Some(args[i]);
                }
            }
        }
        i += 1;
    }

    let template = template.unwrap_or("");

    let ctx = if let Some(t) = target {
        // If a target pane is specified, use its ID in the context
        let pane_id = if t.starts_with('%') {
            t.to_string()
        } else {
            format!("%{t}")
        };
        FormatContext {
            pane_id: Some(pane_id),
            ..Default::default()
        }
    } else {
        FormatContext::default()
    };

    let expanded = format::expand(template, &ctx);

    if print_to_stdout {
        println!("{expanded}");
    }

    0
}
