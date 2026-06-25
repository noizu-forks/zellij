# Zellij ↔ tmux Compatibility (`zellij-cct-tmux`)

The `zellij-cct-tmux` shim translates `tmux` CLI calls into `zellij action`
commands so that tmux-driven tooling — notably Claude Code "agent teams" —
works inside a Zellij session. It is a **command-translation layer, not a
multiplexer**: no own server, no protocol, no UI.

This document captures (1) the current feature-parity picture vs real tmux and
(2) a ready-to-use implementation prompt for closing the remaining gaps.

---

## 1. Feature parity: `zellij-cct-tmux` vs real tmux

### Fully implemented (real behavior + meaningful flags)

| Command (aliases) | Flags honored | vs tmux |
|---|---|---|
| `split-window` | `-c -F -h -l -P -t -v` | good; `-l` size best-effort |
| `send-keys` | `-t -l` | works; **no `-N/-R/-H/-M`**, no repeat count |
| `new-window` | `-c -F -n -P -t` | good; **ignores `-d`** (zellij always focuses new tab); `-b/-a/-k` missing |
| `new-session` | `-d -e -F -n -P -s` | creates zellij session; `-A -t -x -y -c` missing |
| `list-panes` (`lsp`) / `list-windows` (`lsw`) / `list-sessions` (`ls`) | `-F` (+`-t`) | tmux-format output; **no `-a` (all)**; `-f` filter unsupported |
| `capture-pane` (`capturep`) | `-p -e -S -E -t` | dumps via `dump-screen`; **`-b -J -C -N -P` missing** |
| `display-message` | `-p -t` | expands 8 format vars only; no `-c -I -a -v` |
| `resize-pane` | `-D -L -R -U -x -y -t` | good; `-Z` (zoom) / mouse missing |
| `select-pane` | `-P -t -T` | partial; `-L/-R/-U/-D`, `-l`, `-m/-M` missing |
| `select-window`(`selectw`)/`kill-window`/`rename-window`/`kill-pane`/`has-session`/`break-pane`/`join-pane`/`select-layout` | `-t` (+ per-cmd) | basic real impl |

### Format strings (`#{...}`)

Only **8** vars: `pane_id, session_name, window_index, window_id, window_name,
pane_title, socket_path, pid`. tmux has ~250. `#[fg=…]` style sequences pass
through. Conditionals/`#{?…}`, `#{==:}`, `q:`/`b:` modifiers are **unsupported**
(expand to empty).

### Stubbed (accept + exit 0, do nothing)

`kill-server, set-environment(setenv), display-menu, bind-key(bind),
unbind-key(unbind), set-window-option(setw), refresh-client, source-file(source)`.
Any unknown subcommand → same silent `0`.

### Faked / narrow

- `show-options(show)`: hardcodes `focus-events=on`, `mouse=on`; everything else empty.
- `set-option(set)`: parses `-g -p -t -w` but no persisted option store.
- **`-L <socket>` (socket-scoped)**: entirely virtual — `new-session`/`has-session`/
  `kill-server`→0, `display-message`→fabricated `socket_path,pid`. This is the path
  Claude Code uses for its isolated Bash PTYs; it never touches zellij.

### Whole tmux subsystems absent

`attach/detach-session`, `switch-client`, **control mode (`-C`)**, copy-mode +
**paste buffers**, `pipe-pane`, `if-shell`/`run-shell`, `command-prompt`, hooks,
key tables/bindings, `swap/move-window`, `link-window`, `respawn-pane/window`,
`choose-tree`, `wait-for`, `lock`, `clock-mode`, customizable
`base-index`/`pane-base-index`.

### Semantic gaps (correctness, not just coverage) — the ones that bite

1. **Focus-coupled ops need an attached client.** `send-keys`/`capture-pane`/
   `kill-window` to a *window/tab* target go through `go-to-tab-name` + a
   focused-pane action. On a **detached** zellij session these silently no-op —
   the root cause of dropped agent launches. Real tmux drives panes regardless
   of attachment.
2. **No tab→pane-id map.** `capture-pane` of a background tab can only dump the
   *attached client's focused* pane; truly headless per-agent capture isn't
   possible yet (`dump-screen -p terminal_N` works but there is no tab→`terminal_N`
   lookup).
3. **`window_index` = zellij 0-based position**, not tmux's 1-based/`base-index`
   index; shifts on tab move/close (no stable identity except the shim's own `@N`
   winmap).
4. **`kill-window` race** — `go-to-tab-name` then `close-tab` with no sync; can
   close the wrong tab.
5. **Multi-client / `-t` cross-session targeting** unsupported — always the one
   discovered session.

### Bottom line

Parity is **"good enough to spawn and drive a one-pane-per-agent team by
name/`@id` in the attached session, and read it back when attached."** It is
**not** a tmux replacement.

---

## 2. Implementation prompt — closing the parity gaps

```
You are implementing tmux-compatibility features in the Zellij tmux-compat shim
so that Claude Code "agent teams" (and general tmux-driven tooling) work reliably
inside a Zellij session.

## Where you are working
- Crate: 3rd-party/zellij/zellij-cct-tmux  (a git subtree — do NOT push/commit unless asked)
- Entry: src/main.rs → src/lib.rs::run → src/dispatch.rs (subcommand → src/commands/<name>.rs)
- Translation: src/zellij_bridge.rs runs `zellij --session <S> action <...>` subprocesses
- Helpers: src/format.rs (#{...} expansion), src/idmap.rs (tmux %N ↔ zellij terminal_N panes),
  src/winmap.rs (tmux @N window ids ↔ tab names), src/tab_resolve.rs (target → tab),
  src/keys.rs (key translation), src/ready_wait.rs, src/logger.rs
- Build:   cargo build -p zellij-cct-tmux --release   (compiles in ~5s)
- Install: cp -f target/release/zellij-tmux-shim ~/.cargo/bin/zellij-tmux-shim
           (~/.cargo/bin/tmux is already a symlink to it)
- Activation: shim only runs when ZELLIJ_TMUX_COMPAT=1 AND ZELLIJ are set; otherwise execs real tmux.
- Logs: $TMPDIR/zellij-cct/$ZELLIJ_SESSION_NAME/tmux-shim.log (set ZELLIJ_CCT_DEBUG=1)

## Hard constraints (read first)
1. There is a LIVE interactive Zellij session the user is attached to. NEVER create, kill,
   rename, or switch tabs in it for testing. Do all functional testing in a SEPARATE,
   throwaway detached session you create (`zellij -s cct-test ...`) or guard tests so they
   only touch tabs you created and clean them up. Verify which session is attached with
   `zellij --session <S> action list-clients` (a real client row starts with a numeric CLIENT_ID).
2. Preserve the `-L <socket>` socket-scoped path in dispatch.rs::handle_socket_scoped — Claude
   Code uses it for its own Bash PTYs; it must stay a virtual no-op and must not call zellij.
3. Keep existing passing behavior. Add unit tests next to changed code; run `cargo test -p
   zellij-cct-tmux`. Keep `cargo build` warning-clean.

## Critical semantic facts (these are why naive impls fail)
- Focus-based actions (`go-to-tab-name` + `write-chars`/`dump-screen` with NO --pane-id) only
  work when a client is ATTACHED. On a detached session they silently no-op. Prefer targeting
  an explicit pane id (`terminal_N`) so operations work headless.
- `zellij action dump-screen -p terminal_N` returns content with no attached client; without
  -p it dumps only the attached client's focused pane (empty if detached). `--path FILE` writes
  to a file, omitting it prints to STDOUT.
- `zellij action new-tab` prints the new TAB id (a number) on stdout and supports -c/--cwd and
  --name. It does NOT give you the new pane's terminal_N.
- There is currently NO tab→pane-id lookup. `dump-layout` shows tab/pane structure and names
  but not terminal_N ids. Solve this to enable headless per-tab capture/send (see item 1).
- window_index is currently the zellij 0-based tab position, not tmux's base-index-aware index.

## Goal
Raise tmux parity for the agent-teams workflow and common scripted use. Implement in priority
order; after each item: build, install, test in a throwaway session, report pass/fail.

### P0 — reliability of what already "works"
1. Tab→pane-id resolution. Add a way to map a resolved tab to its active pane's terminal_N
   (e.g. record the pane id at new-window/split time into idmap/winmap, and/or parse it from a
   reliable zellij query). Route send-keys and capture-pane through an explicit --pane-id when a
   window/tab target is given, so they work on detached sessions and never disturb user focus.
2. kill-window race: `go-to-tab-name` then `close-tab` can close the wrong tab. Make teardown
   target the intended tab deterministically (close-by-pane-id or verify active tab == target
   before close-tab). Add a test that creates N tabs and kills a specific one.
3. new-window `-d` (do-not-focus): currently ignored; zellij always focuses the new tab. Restore
   the previously-active tab when -d is passed, so scripted creation doesn't steal user focus.

### P1 — coverage gaps that block common scripts
4. Format expansion (src/format.rs): add the high-value vars used by tooling — pane_index,
   window_active, window_panes, session_attached, pane_current_path, pane_pid, history_size,
   cursor_x/cursor_y — and support `#{?cond,a,b}` conditionals and `#{==:x,y}`. Unknown → empty
   (keep current behavior). Unit-test the expander.
5. list-* `-a` (all sessions/windows/panes) and basic `-f` filter support.
6. capture-pane: add `-J` (join wrapped lines), `-N`/`-P`, and correct `-S`/`-E` line-range
   semantics (currently -S maps loosely to --full). Document any zellij limitation.
7. set-option/show-options: back them with a small persisted option store
   (under $TMPDIR/zellij-cct/<session>/options.json) so set/show round-trip instead of being
   faked. Keep focus-events/mouse defaulting to on.

### P2 — broaden surface (only if P0/P1 done)
8. swap-window / move-window / respawn-pane / select-pane directional (-L/-R/-U/-D) / select-pane -l.
9. Paste buffers: set-buffer/show-buffer/load-buffer/save-buffer/paste-buffer backed by a file store.
10. Honor base-index/pane-base-index for window_index/pane_index reporting.

## Out of scope (note, do not attempt)
Control mode (-C), attach/detach-session, copy-mode, hooks, key bindings, if-shell/run-shell,
multi-client targeting. State these remain unsupported.

## Deliverable
- Implement P0 fully and as much of P1 as time allows.
- For each command touched: list the tmux flags now honored and the acceptance test you ran.
- Provide a parity table diff (before → after) for the commands you changed.
- Leave changes uncommitted unless told otherwise; report changed file paths.
- End with a short list of remaining known gaps and any zellij CLI limitations you hit.
```

> Scope note: this prompt targets the implementation work; it does **not** cover
> authoring an eval/regression harness to *measure* parity over time. Continuous
> parity tracking is a separate effort worth commissioning.
