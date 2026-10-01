//! Bring a session's own terminal window to the front.
//!
//! Finding the terminal app by walking the process's parent chain and focusing
//! the exact tab by its tty via `AppleScript` is ported from Claude Command
//! Center v5.14.0 (`_proc_ancestor_terminal`, `focus_terminal_by_tty` in
//! server.py), MIT licence, Copyright (c) 2026 Amir Fish. See
//! `THIRD_PARTY_NOTICES.md`.

use std::process::Command;

/// Process names of terminal apps we know, and their `AppleScript` application names.
const TERMINAL_APPS: &[(&str, &str)] = &[
    ("terminal", "Terminal"),
    ("iterm2", "iTerm2"),
    ("iterm", "iTerm2"),
    ("ghostty", "Ghostty"),
    ("wezterm-gui", "WezTerm"),
    ("wezterm", "WezTerm"),
    ("alacritty", "Alacritty"),
    ("kitty", "kitty"),
    ("warp", "Warp"),
    ("hyper", "Hyper"),
    ("tabby", "Tabby"),
];

/// Match a process command name (possibly a path inside an .app bundle) to a terminal app.
#[must_use]
pub fn terminal_for_comm(comm: &str) -> Option<&'static str> {
    let base = comm
        .rsplit('/')
        .next()
        .unwrap_or(comm)
        .to_lowercase()
        .replace(".app", "");
    TERMINAL_APPS
        .iter()
        .find(|(key, _)| base == *key || base.starts_with(key))
        .map(|(_, app)| *app)
}

fn ps(pid: i32, fields: &str) -> Option<String> {
    let out = Command::new("ps")
        .args(["-o", fields, "-p", &pid.to_string()])
        .output()
        .ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    let line = text.lines().next()?.trim().to_string();
    (!line.is_empty()).then_some(line)
}

/// A process's parent pid and command name. (`ppid` is the keyword macOS `ps` knows.)
fn parent_and_comm(pid: i32) -> Option<(i32, String)> {
    let line = ps(pid, "ppid=,comm=")?;
    let (parent, comm) = line.split_once(char::is_whitespace)?;
    Some((parent.trim().parse().ok()?, comm.trim().to_string()))
}

/// The terminal app hosting a process: walk up its parents until one is a known terminal.
#[must_use]
pub fn terminal_app_of(pid: i32) -> Option<&'static str> {
    let mut current = pid;
    for _ in 0..20 {
        let (parent, comm) = parent_and_comm(current)?;
        if let Some(app) = terminal_for_comm(&comm) {
            return Some(app);
        }
        if parent <= 1 {
            return None;
        }
        current = parent;
    }
    None
}

/// The parent of a process, if it is still running.
#[must_use]
pub fn parent_pid(pid: i32) -> Option<i32> {
    ps(pid, "ppid=")?.parse().ok()
}

/// The controlling tty of a process, e.g. `/dev/ttys008`.
#[must_use]
pub fn tty_of(pid: i32) -> Option<String> {
    let tty = ps(pid, "tty=")?;
    (tty != "??" && tty != "-").then(|| format!("/dev/{}", tty.trim_start_matches("/dev/")))
}

/// `AppleScript` that selects the tab owning `tty` and brings its window forward.
#[must_use]
pub fn focus_script(app: &str, tty: &str) -> String {
    match app {
        "iTerm2" => format!(
            r#"tell application "iTerm2"
  set found to false
  repeat with i from 1 to count of windows
    try
      set w to window i
      repeat with j from 1 to count of tabs of w
        try
          set t to tab j of w
          repeat with s in sessions of t
            if tty of s is "{tty}" then
              select w
              tell w to select t
              select s
              set found to true
              exit repeat
            end if
          end repeat
          if found then exit repeat
        end try
      end repeat
      if found then exit repeat
    end try
  end repeat
  if found then
    activate
    return "ok"
  end if
  return "notfound"
end tell"#
        ),
        "Terminal" => format!(
            r#"tell application "Terminal"
  set foundWin to missing value
  set foundTab to missing value
  repeat with i from 1 to count of windows
    try
      set w to window i
      repeat with j from 1 to count of tabs of w
        try
          set t to tab j of w
          if tty of t is "{tty}" then
            set foundWin to w
            set foundTab to t
            exit repeat
          end if
        end try
      end repeat
      if foundTab is not missing value then exit repeat
    end try
  end repeat
  if foundTab is missing value then return "notfound"
  set selected of foundTab to true
  try
    set index of foundWin to 1
  end try
  activate
  return "ok"
end tell"#
        ),
        // No tab-level scripting: the best we can do is bring the app forward.
        other => format!("tell application \"{other}\" to activate\nreturn \"ok\""),
    }
}

fn osascript(script: &str) -> Result<String, String> {
    let out = Command::new("osascript")
        .args(["-e", script])
        .output()
        .map_err(|e| format!("could not run osascript: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Bring the terminal window running `pid` to the front.
pub fn focus_pid(pid: i32) -> Result<(), String> {
    let app = terminal_app_of(pid).ok_or("its terminal app could not be identified")?;
    let script = match tty_of(pid) {
        Some(tty) => focus_script(app, &tty),
        None => format!("tell application \"{app}\" to activate\nreturn \"ok\""),
    };
    match osascript(&script)?.as_str() {
        "ok" => Ok(()),
        _ => Err(format!("no {app} tab owns that session any more")),
    }
}

/// The program a process is running now, by file name (`-zsh` for a login zsh reads `zsh`).
#[must_use]
pub fn process_name(pid: i32) -> Option<String> {
    let comm = ps(pid, "comm=")?;
    let name = comm.rsplit('/').next().unwrap_or(&comm);
    Some(name.trim_start_matches('-').to_string())
}

/// Escape for an `AppleScript` string literal.
#[must_use]
pub fn applescript_literal(s: &str) -> String {
    s.replace('\\', r"\\").replace('"', "\\\"")
}

/// macOS's own folder picker. None when the human cancels.
pub fn choose_folder(prompt: &str) -> Result<Option<String>, String> {
    let script = format!(
        "POSIX path of (choose folder with prompt \"{}\")",
        applescript_literal(prompt)
    );
    let out = Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| format!("could not run osascript: {e}"))?;
    if out.status.success() {
        let path = String::from_utf8_lossy(&out.stdout)
            .trim()
            .trim_end_matches('/')
            .to_string();
        return Ok((!path.is_empty()).then_some(path));
    }
    let err = String::from_utf8_lossy(&out.stderr);
    // -128 is AppleScript's "User canceled".
    if err.contains("-128") {
        Ok(None)
    } else {
        Err(err.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_live_process_parent_and_name() {
        let me = i32::try_from(std::process::id()).expect("pid fits");
        let (parent, comm) = parent_and_comm(me).expect("ps reads this process");
        assert_eq!(Some(parent), parent_pid(me));
        assert!(!comm.is_empty());
    }

    #[test]
    fn recognises_terminal_processes() {
        assert_eq!(
            terminal_for_comm("/Applications/iTerm.app/Contents/MacOS/iTerm2"),
            Some("iTerm2")
        );
        assert_eq!(
            terminal_for_comm(
                "/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal"
            ),
            Some("Terminal")
        );
        assert_eq!(terminal_for_comm("ghostty"), Some("Ghostty"));
        assert_eq!(terminal_for_comm("zsh"), None);
    }

    #[test]
    fn scripts_target_the_tty() {
        assert!(focus_script("iTerm2", "/dev/ttys004").contains("tty of s is \"/dev/ttys004\""));
        assert!(focus_script("Terminal", "/dev/ttys004").contains("tty of t is \"/dev/ttys004\""));
        assert_eq!(
            focus_script("Warp", "/dev/ttys004"),
            "tell application \"Warp\" to activate\nreturn \"ok\""
        );
    }

    #[test]
    fn quoting() {
        assert_eq!(applescript_literal(r#"say "hi" \o/"#), r#"say \"hi\" \\o/"#);
    }

    #[test]
    fn this_process_has_a_parent_chain() {
        // Walking our own ancestry must terminate (terminal or not).
        let _ = terminal_app_of(i32::try_from(std::process::id()).unwrap());
        assert!(ps(i32::try_from(std::process::id()).unwrap(), "pid=").is_some());
        assert!(parent_pid(i32::try_from(std::process::id()).unwrap()).is_some_and(|p| p > 0));
        let me = process_name(i32::try_from(std::process::id()).unwrap()).expect("name");
        assert!(me.starts_with("office_building"), "got {me}");
    }
}
