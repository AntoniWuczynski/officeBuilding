//! Carrying a handover out: everything that can be done before anything is ended, then one
//! polite stop of the old session and the start of the receiver in the same folder. The
//! third exception to OB-027: one stop, a bounded wait, never a second signal or force.

use super::super::launch::{
    DEFAULT_EFFORT, DEFAULT_MODEL, Launched, chosen_model, hireable, launch_command,
};
use super::{Failure, HandoverHost, NOT_SET_UP, Office, Plan, now};
use crate::discovery::claude_code::chat::ChatReader;
use crate::handover::{Work, brief, git, start};
use crate::model::{DeskView, HireRequest, ModelOption, ToolKind, ToolOptions};
use crate::process;
use crate::pty;
use crate::settings::StartWith;
use crate::setup::Launch;
use std::path::PathBuf;
use std::time::Duration;

const NOT_INSTALLED: &str = "Codex is not installed on this Mac";
const CHANGED: &str = "the desk changed while it was being handed over";
pub(super) const DID_NOT_STOP: &str = "Claude Code did not stop when asked, so Codex was not started. Type /exit in its terminal, then hand the desk over again";
pub(super) const ENDED_LATE: &str = "Claude Code ended only after the handover had stopped waiting for it, so Codex was not started. Resume the Claude Code session from the sign-out sheet";

/// What `prepare` made: the command that starts the receiver, and the brief it reads.
struct Prepared {
    command: String,
    brief: PathBuf,
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
            Ok(prepared) => self
                .change_hands(host, plan, &prepared.command)
                .inspect_err(|_| brief::remove(&prepared.brief)),
        };
        self.finish(plan, id, result)
    }

    /// Everything that can be done before anything is ended. The office lock is held only
    /// to read the feed for the launch options.
    fn prepare(&self, host: &HandoverHost, plan: &Plan, id: &str) -> Result<Prepared, String> {
        if !plan.folder.is_dir() {
            return Err(super::FOLDER_GONE.to_string());
        }
        let options = hireable(self.hire_options(), ToolKind::Codex)?;
        let shell = pty::login_shell();
        let paths = process::login_paths(&*host.runner, &shell, ["codex", "git"]);
        let named = |name: &str| {
            paths
                .iter()
                .find(|p| p.file_name().is_some_and(|f| f == name))
        };
        let codex = named("codex").ok_or(NOT_INSTALLED)?;
        start::signed_in(&*host.runner, codex)?;
        let mut reader = ChatReader::new(&self.home, &plan.session_id);
        reader.read();
        let git = git::summary(
            &*host.runner,
            named("git").map(PathBuf::as_path),
            &plan.folder,
        );
        let text = brief::render(
            &brief::Facts {
                id,
                from: plan.record.tool,
                to: plan.record.tool.other(),
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
                earlier: reader.earlier(),
            },
            &reader.kept(),
        );
        let brief = brief::write(&host.briefs, id, &text, &plan.folder)?;
        let prompt = start::first_prompt(&plan.title, plan.record.tool, &brief);
        self.setup
            .launch(ToolKind::Codex, &self.home, Some(&self.lock().feed))
            .and_then(|setup| codex_command(&plan.start, &options, &prompt, &shell, &setup))
            .map(|command| Prepared {
                command,
                brief: brief.clone(),
            })
            .inspect_err(|_| brief::remove(&brief))
    }

    /// End the old session and start the receiver. Ok: the receiver's terminal.
    pub(super) fn change_hands(
        &self,
        host: &HandoverHost,
        plan: &Plan,
        command: &str,
    ) -> Result<String, Failure> {
        let changed = || Failure {
            reason: CHANGED.to_string(),
            ended: false,
        };
        {
            let inner = self.lock();
            // The proof and the work are checked again: the limit may have reset, and the
            // session carried on with a workflow, while the receiver was being prepared.
            let still = inner.owned.get(&plan.terminal) == Some(&plan.session_id)
                && !Self::fixed_role(&inner, &plan.terminal)
                && {
                    let (book, at) = (&inner.handover.book, now());
                    book.own(&plan.session_id, at)
                        .or_else(|| book.live(plan.record.tool, at))
                }
                .is_some_and(|r| r.same_limit(plan.record.reset_at, plan.record.seen_at))
                && (plan.by_human
                    || matches!(inner.handover.works.of(&plan.session_id), Work::Clear));
            if !still {
                return Err(changed());
            }
        }
        if matches!(
            self.terminals.wait_exit(&plan.terminal, Duration::ZERO),
            Ok(None)
        ) {
            let pid = self.terminals.pid(&plan.terminal).map_err(|_| changed())?;
            // Asked once. One that outlives the wait is left to A Human, and Codex is not started.
            (host.stop)(pid);
            if matches!(
                self.terminals.wait_exit(&plan.terminal, host.stop_wait),
                Ok(None)
            ) {
                return Err(Failure {
                    reason: DID_NOT_STOP.to_string(),
                    ended: false,
                });
            }
        }
        self.launch(
            &plan.folder,
            command,
            Launched::Discovered {
                title: plan.title.clone(),
            },
        )
        .map_err(|why| Failure {
            reason: format!(
                "Claude Code was ended and Codex did not start: {}. Resume the Claude Code session from the sign-out sheet",
                without_tail(&why)
            ),
            ended: true,
        })
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

/// The model and effort ids the receiving tool is launched with. The tool's own default
/// passes neither. A choice must be one the tool itself lists.
pub(super) fn receiver(
    start: &StartWith,
    options: &ToolOptions,
) -> Result<(ModelOption, String), String> {
    match start {
        StartWith::ToolDefault => Ok((
            ModelOption {
                id: DEFAULT_MODEL.to_string(),
                label: DEFAULT_MODEL.to_string(),
                efforts: Vec::new(),
                default_effort: DEFAULT_EFFORT.to_string(),
            },
            DEFAULT_EFFORT.to_string(),
        )),
        StartWith::Chosen { model, effort } => {
            // `launch_command` writes the effort inside a TOML string.
            if effort.contains(['"', '\\']) {
                return Err("that effort cannot be passed to Codex".to_string());
            }
            let (model, effort) = chosen_model(options, Some(model), Some(effort))?;
            Ok((model.clone(), effort))
        }
    }
}

/// The command that starts Codex on `prompt`: the building's ordinary Codex hire, never routed.
pub(super) fn codex_command(
    start: &StartWith,
    options: &ToolOptions,
    prompt: &str,
    shell: &str,
    setup: &Launch,
) -> Result<String, String> {
    let (model, effort) = receiver(start, options)?;
    let req = HireRequest {
        tool: ToolKind::Codex,
        title: prompt.to_string(),
        model: model.id.clone(),
        effort,
        ultracode: false,
        view: DeskView::Terminal,
    };
    launch_command(&req, &model, prompt, shell, setup)
}
