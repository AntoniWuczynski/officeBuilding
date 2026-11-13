//! Carrying a handover out: everything that can be done before anything is ended, then one
//! polite stop of the old session and the start of the receiver in the same folder. The
//! third exception to OB-027, for either tool: one stop, a bounded wait, never a second
//! signal or force.

use super::super::launch::{
    DEFAULT_EFFORT, DEFAULT_MODEL, Launched, chosen_model, hireable, launch_command,
};
use super::{Failure, HandoverHost, NOT_SET_UP, Office, Plan, desk, now};
use crate::handover::{Tool, Work, brief, git, start};
use crate::model::{DeskView, HireRequest, ModelOption, ToolOptions};
use crate::process;
use crate::pty;
use crate::routing::router::Opened;
use crate::settings::StartWith;
use crate::setup::{Launch, Opt};
use crate::tools;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(super) const CHANGED: &str = "the desk changed while it was being handed over";

/// The old session outlived the wait after the one stop.
pub(super) fn did_not_stop(from: Tool) -> String {
    format!(
        "{} did not stop when asked, so {} was not started. Type /exit in its terminal, then hand the desk over again",
        from.label(),
        from.other().label()
    )
}

/// The old session that did not stop has ended since.
pub(super) fn ended_late(from: Tool) -> String {
    format!(
        "{} ended only after the handover had stopped waiting for it, so {} was not started. Resume the {} session from the sign-out sheet",
        from.label(),
        from.other().label(),
        from.label()
    )
}

/// The old session was ended and the receiver did not start, for the reason `why`.
fn did_not_start(from: Tool, why: &str) -> String {
    format!(
        "{} was ended and {} did not start: {why}. Resume the {} session from the sign-out sheet",
        from.label(),
        from.other().label(),
        from.label()
    )
}

/// What `prepare` made: the command that starts the receiver, the brief it reads, and the
/// route opened for it on a routed floor.
struct Prepared {
    command: String,
    brief: PathBuf,
    route: Option<Opened>,
}

impl Office {
    /// Carry `plan` out and record how it went. Err: why it did not work, in words.
    pub(in crate::office) fn carry_out_handover(&self, plan: &Plan) -> Result<(), String> {
        let Some(host) = &self.handover_host else {
            self.lock().handover.underway.remove(&plan.session_id);
            return Err(NOT_SET_UP.to_string());
        };
        // No office lock: the briefs folder is read here.
        let top = brief::highest(&host.briefs);
        let id = {
            let mut inner = self.lock();
            inner.handover.book.at_least(top.saturating_add(1));
            inner.handover.book.next_id()
        };
        let result = match self.prepare(host, plan, &id) {
            Err(reason) => Err(Failure {
                reason,
                ended: false,
            }),
            Ok(Prepared {
                command,
                brief: path,
                route,
            }) => self
                .change_hands(host, plan, &command, route)
                .inspect_err(|_| brief::remove(&path)),
        };
        self.finish(plan, id, result)
    }

    /// Everything that can be done before anything is ended. The office lock is held only
    /// to read the feed for the launch options, and by `routed` for a moment. A route is
    /// opened last, so no failure here can leave one open.
    fn prepare(&self, host: &HandoverHost, plan: &Plan, id: &str) -> Result<Prepared, String> {
        let (from, to) = (plan.record.tool, plan.record.tool.other());
        if !plan.folder.is_dir() {
            return Err(super::FOLDER_GONE.to_string());
        }
        let options = hireable(self.hire_options(), to.kind())?;
        let not_installed = || format!("{} is not installed on this Mac", to.label());
        let binary = tools::binary(to.kind()).ok_or_else(not_installed)?;
        let shell = pty::login_shell();
        let paths = process::login_paths(&*host.runner, &shell, [binary, "git"]);
        let named = |name: &str| {
            paths
                .iter()
                .find(|p| p.file_name().is_some_and(|f| f == name))
        };
        let program = named(binary).ok_or_else(not_installed)?;
        start::signed_in(&*host.runner, to, program)?;
        let (entries, earlier) = desk::conversation(from, &self.home, &plan.session_id);
        let git = git::summary(
            &*host.runner,
            named("git").map(PathBuf::as_path),
            &plan.folder,
        );
        let text = brief::render(
            &brief::Facts {
                id,
                from,
                to,
                by_human: plan.by_human,
                at: now(),
                window: plan.record.window,
                reset_at: plan.record.reset_at,
                title: &plan.title,
                folder: &plan.folder,
                floor: &plan.floor,
                branch: plan.branch.as_deref(),
                worktree: plan.worktree,
                work: &plan.work,
                git: &git,
                earlier,
            },
            &entries,
        );
        let brief = brief::write(&host.briefs, id, &text, &plan.folder)?;
        // The path asked for and the file it resolves to are the same (V13).
        let told = std::fs::canonicalize(&brief).unwrap_or_else(|_| brief.clone());
        let prompt = start::first_prompt(&plan.title, from, &told);
        // Two statements: `routed` takes the office lock, which the feed's guard holds.
        let setup = self
            .setup
            .launch(to.kind(), &self.home, Some(&self.lock().feed));
        setup
            .and_then(|setup| {
                let setup = with_leave(to, setup, &told);
                receiver_command(to, &plan.start, &options, &prompt, &shell, &setup)
            })
            .and_then(|command| self.routed(&plan.project_id, to.kind(), false, command, &shell))
            .map(|(command, route)| Prepared {
                command,
                brief: brief.clone(),
                route,
            })
            .inspect_err(|_| brief::remove(&brief))
    }

    /// End the old session and start the receiver. Ok: the receiver's terminal. `route`, the
    /// receiver's route on a routed floor, is closed on every failure.
    pub(super) fn change_hands(
        &self,
        host: &HandoverHost,
        plan: &Plan,
        command: &str,
        route: Option<Opened>,
    ) -> Result<String, Failure> {
        let from = plan.record.tool;
        let refused = |reason: String| {
            if let (Some(opened), Some(router)) = (&route, &self.routing) {
                router.close(&opened.id);
            }
            Failure {
                reason,
                ended: false,
            }
        };
        {
            let inner = self.lock();
            // The proof and the work are checked again: the limit may have reset, and the
            // session carried on with background work, while the receiver was being prepared.
            let still = inner.owned.get(&plan.terminal) == Some(&plan.session_id)
                && !Self::fixed_role(&inner, &plan.terminal)
                && {
                    let (book, at) = (&inner.handover.book, now());
                    book.own(&plan.session_id, at)
                        .or_else(|| book.live(from, at))
                }
                .is_some_and(|r| r.same_limit(plan.record.reset_at, plan.record.seen_at))
                && (plan.by_human
                    || matches!(inner.handover.works.of(&plan.session_id), Work::Clear));
            if !still {
                drop(inner);
                return Err(refused(CHANGED.to_string()));
            }
        }
        if matches!(
            self.terminals.wait_exit(&plan.terminal, Duration::ZERO),
            Ok(None)
        ) {
            let Ok(pid) = self.terminals.pid(&plan.terminal) else {
                return Err(refused(CHANGED.to_string()));
            };
            // Asked once. One that outlives the wait is left to A Human, and the receiver is
            // not started.
            (host.stop)(pid);
            if matches!(
                self.terminals.wait_exit(&plan.terminal, host.stop_wait),
                Ok(None)
            ) {
                return Err(refused(did_not_stop(from)));
            }
        }
        // `launch_routed` closes the route itself when the launch fails.
        let terminal = self
            .launch_routed(
                &plan.folder,
                command,
                Launched::Discovered {
                    title: plan.title.clone(),
                },
                route,
            )
            .map_err(|why| Failure {
                reason: did_not_start(from, without_tail(&why)),
                ended: true,
            })?;
        self.note_view(from.other().kind(), &terminal);
        Ok(terminal)
    }
}

/// `why` without the receiver terminal's last lines, which can hold anything the shell echoed.
/// They are kept out of what is saved and sent in every snapshot.
fn without_tail(why: &str) -> &str {
    if why.starts_with("the agent stopped as soon as it started") {
        why.split_once(": ").map_or(why, |(head, _)| head)
    } else {
        why
    }
}

/// The model and effort the receiving tool is launched with. A choice must be one the tool
/// lists. Codex's own default passes neither. Claude Code's is the model the hire dialog
/// preselects, at that model's default effort (K6).
pub(super) fn receiver(
    to: Tool,
    start: &StartWith,
    options: &ToolOptions,
) -> Result<(ModelOption, String), String> {
    let (model, effort) = match (to, start) {
        (Tool::Codex, StartWith::ToolDefault) => {
            return Ok((
                ModelOption {
                    id: DEFAULT_MODEL.to_string(),
                    label: DEFAULT_MODEL.to_string(),
                    efforts: Vec::new(),
                    default_effort: DEFAULT_EFFORT.to_string(),
                },
                DEFAULT_EFFORT.to_string(),
            ));
        }
        (Tool::ClaudeCode, StartWith::ToolDefault) => chosen_model(options, None, None)?,
        (_, StartWith::Chosen { model, effort }) => {
            // `launch_command` writes Codex's effort inside a TOML string.
            if to == Tool::Codex && effort.contains(['"', '\\']) {
                return Err("that effort cannot be passed to Codex".to_string());
            }
            chosen_model(options, Some(model), Some(effort))?
        }
    };
    Ok((model.clone(), effort))
}

/// The one read rule Claude Code gets for `brief` (K7). Codex's setup is returned as it is,
/// and so is Claude Code's when the path can carry no rule.
pub(super) fn with_leave(to: Tool, mut setup: Launch, brief: &Path) -> Launch {
    if to == Tool::ClaudeCode
        && let Some(rule) = start::read_rule(brief)
    {
        setup.opts.push(Opt {
            flag: "--allowedTools",
            value: rule,
        });
    }
    setup
}

/// The command that starts `to` on `prompt`: the building's ordinary hire of that tool.
pub(super) fn receiver_command(
    to: Tool,
    start: &StartWith,
    options: &ToolOptions,
    prompt: &str,
    shell: &str,
    setup: &Launch,
) -> Result<String, String> {
    let (model, effort) = receiver(to, start, options)?;
    let req = HireRequest {
        tool: to.kind(),
        title: prompt.to_string(),
        model: model.id.clone(),
        effort,
        ultracode: to == Tool::ClaudeCode && options.ultracode == Some(true),
        view: DeskView::Terminal,
    };
    launch_command(&req, &model, prompt, shell, setup)
}
