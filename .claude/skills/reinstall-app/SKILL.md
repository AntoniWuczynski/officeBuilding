---
name: reinstall-app
description: Use when asked to rebuild, reinstall or restart the Office Building desktop app ("reinstall the app", "ship it to my Applications", "restart Office Building with the new build"). Quits the running app, builds a release bundle, installs it in ~/Applications and opens it again. Run it from a session outside the app, since quitting the app ends any terminal running inside it.
---

# Rebuild, reinstall and restart Office Building

Run from the officeBuilding repo root, in a session **outside** the app. Quitting
the app ends every agent running in one of its in-app terminals, including the
one running this skill.

The app: `~/Applications/Office Building.app`, bundle id `com.wuczynski.office-building`.
The build and copy are `pnpm app:install` (package.json).

## Steps

1. **Check where you are.** Make sure the working directory is the repo root (it
   has `src-tauri/tauri.conf.json`). If it is not, stop and say so. Note `git
   log -1 --oneline` and whether `git status --short` shows changes: the build
   takes the working tree as it is, committed or not.

2. **Make sure this session is not inside the app.** Walk up from the shell's
   process to the root; if any ancestor is the app, stop and tell the human to
   run this from a terminal outside Office Building:
   ```sh
   p=$$; while [ "$p" -gt 1 ]; do
     ps -o comm= -p "$p" | grep -q "Office Building.app/Contents/MacOS/office-building" && echo INSIDE
     p=$(ps -o ppid= -p "$p" | tr -d ' ')
   done
   ```

3. **Say what will stop.** Check whether the app is running (`pgrep -f` does not
   see it on macOS, so ask by bundle id):
   ```sh
   osascript -e 'application id "com.wuczynski.office-building" is running'
   ```
   If it is, tell the human that quitting the app stops agents in its in-app
   terminals and its desks without a terminal (hand-started terminals outside
   the app keep running). Ask before going on if they have not already said to
   go ahead in this request.

4. **Quit the app cleanly** (skip when it is not running).
   ```sh
   osascript -e 'tell application id "com.wuczynski.office-building" to quit'
   ```
   Poll the `is running` check above every second for up to 20 s. If it is
   still running after that, ask the human before `kill`ing it (its pid:
   `lsappinfo info -only pid -app com.wuczynski.office-building`). Never
   force-quit without asking.

5. **Build and install, detached.** A release build takes minutes, so run it
   with a log file instead of in the foreground, then poll the log:
   ```sh
   LOG="${TMPDIR:-/tmp}/office-building-install.log"
   nohup pnpm app:install > "$LOG" 2>&1 &
   ```
   Poll until the `pnpm` process has exited, then check its outcome: the log
   ends without `error` lines from `tauri build`, and
   `~/Applications/Office Building.app` exists with a fresh modification time.
   On failure, show the last 30 lines of the log and stop. A failed build
   leaves the old app in place (the copy runs only after a good build); if the
   copy itself failed, say plainly that the app may now be missing.

6. **Open it again.**
   ```sh
   open "$HOME/Applications/Office Building.app"
   ```
   Poll the `is running` check for up to 15 s to confirm it started.

7. **Report.** One short paragraph: the commit built (and whether uncommitted
   changes went in), that the app was reinstalled and is running again, how
   long the build took, and the log path. If anything was skipped or failed,
   say which step and why.

## Notes

- Do not run `pnpm tauri:dev` at the same time: the dev app shares the same data
  folder (`~/Library/Application Support/com.wuczynski.office-building`).
- The skill never deletes app data. Only the app bundle in `~/Applications` is replaced.
