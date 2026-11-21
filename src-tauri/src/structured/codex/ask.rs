//! Codex's approval requests turned into what the human sees: a title, a
//! detail and choices, each choice carrying the reply it sends back. Choices
//! follow what each request type allows in the protocol (see `wire.rs`).

use super::requests::Class;
use super::wire::{
    CommandAction, CommandApprovalKind, CommandApprovalParams, CommandDecision, ElicitAction,
    Elicitation, ElicitationHead, FileApprovalParams, FileSystemPermissions, FormSchema, FsAccess,
    FsEntry, FsPath, FsSpecial, GrantScope, LegacyExecParams, LegacyFileChange, LegacyPatchParams,
    NetworkPermissions, Permissions, PermissionsParams, ReplyBody, ReviewDecision, RuleAction,
    UserInputParams,
};
use crate::structured::{Field, FieldKind};
use std::collections::BTreeMap;

/// An approval about to open: what it shows, the fields the human fills in,
/// and per choice its label, whether it lets the agent go ahead, and the reply
/// it sends. With a `fill`, the accepting reply is built from the fields' values.
pub(super) struct Ask {
    pub title: String,
    pub detail: String,
    pub item: Option<String>,
    pub fields: Vec<Field>,
    pub fill: Option<Fill>,
    pub choices: Vec<(String, bool, ReplyBody)>,
    /// The agent keeps working while this is open (a non-blocking question).
    pub background: bool,
    /// Which of Codex's waiting flags it counts under.
    pub class: Class,
}

/// How an accepting reply is built from the human's values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Fill {
    /// `request_user_input`: one answer per question, by question id.
    Input(Vec<String>),
    /// An MCP form: each field's value in the JSON type its schema asks for.
    Form(Vec<(String, FormKind)>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FormKind {
    Text,
    Number,
    Integer,
    Boolean,
}

fn command_choice(d: &CommandDecision) -> (String, bool) {
    match d {
        CommandDecision::Accept => ("Allow once".to_string(), true),
        CommandDecision::AcceptForSession => ("Allow for this session".to_string(), true),
        CommandDecision::AcceptWithExecpolicyAmendment {
            execpolicy_amendment,
        } => (
            format!("Always allow `{}`", execpolicy_amendment.join(" ")),
            true,
        ),
        CommandDecision::ApplyNetworkPolicyAmendment {
            network_policy_amendment: a,
        } => match a.action {
            RuleAction::Allow => (format!("Always allow {}", a.host), true),
            RuleAction::Deny => (format!("Always block {}", a.host), false),
        },
        CommandDecision::Decline => ("Decline".to_string(), false),
        CommandDecision::Cancel => ("Decline and stop the turn".to_string(), false),
    }
}

/// A permission request lists every part of what it asks, in the order Codex
/// gives it. A grant echoes the request exactly, so it is offered only when
/// every part could be read and listed. Otherwise it can only be declined.
pub(super) fn permissions_approval(p: PermissionsParams, cwd: &str) -> Ask {
    let place = p.cwd.filter(|c| c != cwd).map(|c| format!("in {c}"));
    let scope = scope(&p.permissions);
    let grant = |scope| ReplyBody::Permissions {
        permissions: p.permissions.clone(),
        scope,
    };
    let none = (
        "Decline".to_string(),
        false,
        ReplyBody::Permissions {
            permissions: Permissions {
                network: None,
                file_system: None,
            },
            scope: GrantScope::Turn,
        },
    );
    let (asked, choices) = match scope {
        Some(asked) => (
            asked,
            vec![
                (
                    "Allow for this turn".to_string(),
                    true,
                    grant(GrantScope::Turn),
                ),
                (
                    "Allow for this session".to_string(),
                    true,
                    grant(GrantScope::Session),
                ),
                none,
            ],
        ),
        None => (vec![UNREADABLE.to_string()], vec![none]),
    };
    Ask {
        fields: Vec::new(),
        fill: None,
        title: "Give Codex extra permissions?".to_string(),
        detail: lines(
            [p.reason, place]
                .into_iter()
                .chain(asked.into_iter().map(Some)),
        ),
        item: Some(p.item_id),
        choices,
        background: false,
        class: Class::Approval,
    }
}

const UNREADABLE: &str = "The building could not read everything this request asks for, so it can only be declined here.";

/// One line per part of a permission request, or `None` when a part could
/// not be read.
fn scope(p: &Permissions) -> Option<Vec<String>> {
    let mut out = Vec::new();
    if let Some(network) = &p.network {
        match serde_json::from_str::<NetworkPermissions>(network.get())
            .ok()?
            .enabled
        {
            Some(true) => out.push("Network access".to_string()),
            Some(false) => out.push("No network access".to_string()),
            None => {}
        }
    }
    if let Some(fs) = &p.file_system {
        let fs: FileSystemPermissions = serde_json::from_str(fs.get()).ok()?;
        for entry in fs.entries.into_iter().flatten() {
            out.push(entry_line(entry)?);
        }
        for (label, paths) in [("Read", fs.read), ("Write", fs.write)] {
            if let Some(paths) = paths.filter(|p| !p.is_empty()) {
                out.push(format!("{label}: {}", paths.join(", ")));
            }
        }
        if let Some(depth) = fs.glob_scan_max_depth {
            out.push(format!("Patterns are matched up to {depth} folders deep"));
        }
    }
    Some(out)
}

/// `"<access>: <what>"`, or `None` for an access, path or special path the
/// building does not know.
fn entry_line(entry: FsEntry) -> Option<String> {
    let access = match entry.access {
        FsAccess::Read => "Read",
        FsAccess::Write => "Write",
        FsAccess::Deny => "No access",
        FsAccess::Other => return None,
    };
    let what = match entry.path {
        FsPath::Path { path } => path,
        FsPath::GlobPattern { pattern } => format!("every path matching {pattern}"),
        FsPath::Special { value } => match value {
            FsSpecial::Root => "the whole disk".to_string(),
            FsSpecial::Minimal => "the system's minimal paths".to_string(),
            FsSpecial::Tmpdir => "the temporary folder".to_string(),
            FsSpecial::SlashTmp => "/tmp".to_string(),
            FsSpecial::ProjectRoots { subpath } => subpath.map_or_else(
                || "the project folders".to_string(),
                |s| format!("{s} in the project folders"),
            ),
            FsSpecial::Unknown { path, subpath } => match subpath {
                Some(s) => format!("{path} ({s})"),
                None => path,
            },
            FsSpecial::Other => return None,
        },
        FsPath::Other => return None,
    };
    Some(format!("{access}: {what}"))
}

/// Codex's questions, one field each with its own prompt, options and masked
/// or free-text answer, sent together with "Send" under their own ids.
pub(super) fn user_input(p: UserInputParams) -> Option<Ask> {
    let several = p.questions.len() > 1;
    let title = match p.questions.as_slice() {
        [] => return None,
        [q] => q.text.clone(),
        all => format!("Codex has {} questions", all.len()),
    };
    let ids: Vec<String> = p.questions.iter().map(|q| q.id.clone()).collect();
    let fields = p
        .questions
        .into_iter()
        .map(|q| {
            let (label, prompt) = if !q.header.is_empty() {
                (q.header, several.then_some(q.text))
            } else if several {
                (q.text, None)
            } else {
                ("Your answer".to_string(), None)
            };
            let options = q.options.unwrap_or_default();
            let detail = lines(std::iter::once(prompt).chain(options.iter().map(|o| {
                (!o.description.is_empty()).then(|| format!("{}: {}", o.label, o.description))
            })));
            let kind = match (options.is_empty(), q.is_secret) {
                (false, secret) => FieldKind::Pick {
                    options: options.into_iter().map(|o| o.label).collect(),
                    other: q.is_other,
                    secret,
                },
                (true, true) => FieldKind::Secret,
                (true, false) => FieldKind::Text,
            };
            Field {
                id: q.id,
                label,
                detail,
                kind,
                required: true,
            }
        })
        .collect();
    let send = ReplyBody::Input {
        answers: BTreeMap::new(),
    };
    Some(Ask {
        title,
        detail: String::new(),
        item: None,
        fields,
        fill: Some(Fill::Input(ids)),
        choices: vec![("Send".to_string(), true, send)],
        background: !p.is_blocking.unwrap_or(true),
        class: Class::Input,
    })
}

/// A URL elicitation can be accepted as it is; a form's fields become the
/// approval's fields, sent with "Accept".
pub(super) fn elicitation(params: &str) -> Result<Ask, String> {
    let head: ElicitationHead = serde_json::from_str(params).map_err(|e| e.to_string())?;
    let mode: Elicitation = serde_json::from_str(params).map_err(|e| e.to_string())?;
    let elicit = |action, content| ReplyBody::Elicit {
        action,
        content,
        meta: None,
    };
    let (detail, accept, fields, fill) = match mode {
        Elicitation::Url { message, url } => {
            (lines([Some(message), Some(url)]), true, Vec::new(), None)
        }
        Elicitation::Form {
            message,
            requested_schema:
                FormSchema {
                    properties,
                    required,
                },
        } => {
            let mut fields = Vec::new();
            let mut kinds = Vec::new();
            for (name, field) in properties {
                let (kind, form) = match (field.kind.as_deref(), field.one_of) {
                    (_, Some(options)) => (
                        FieldKind::Pick {
                            options,
                            other: false,
                            secret: false,
                        },
                        FormKind::Text,
                    ),
                    (Some("number"), None) => (FieldKind::Number, FormKind::Number),
                    (Some("integer"), None) => (FieldKind::Number, FormKind::Integer),
                    (Some("boolean"), None) => (FieldKind::Boolean, FormKind::Boolean),
                    _ => (FieldKind::Text, FormKind::Text),
                };
                let label = lines([
                    field.title.or_else(|| Some(name.clone())),
                    field.description,
                ]);
                fields.push(Field {
                    required: required.contains(&name),
                    id: name.clone(),
                    label,
                    detail: String::new(),
                    kind,
                });
                kinds.push((name, form));
            }
            (message, true, fields, Some(Fill::Form(kinds)))
        }
        Elicitation::OpenaiForm { message } => (message, false, Vec::new(), None),
    };
    let mut choices = Vec::new();
    if accept {
        choices.push((
            "Accept".to_string(),
            true,
            elicit(ElicitAction::Accept, Some(BTreeMap::new())),
        ));
    }
    choices.push((
        "Decline".to_string(),
        false,
        elicit(ElicitAction::Decline, None),
    ));
    choices.push((
        "Cancel".to_string(),
        false,
        elicit(ElicitAction::Cancel, None),
    ));
    Ok(Ask {
        title: format!("{} asks for your input", head.server_name),
        detail,
        item: None,
        fields,
        fill,
        choices,
        background: false,
        class: Class::Approval,
    })
}

fn legacy_choices() -> Vec<(String, bool, ReplyBody)> {
    [
        ("Allow once", true, ReviewDecision::Approved),
        (
            "Allow for this session",
            true,
            ReviewDecision::ApprovedForSession,
        ),
        (
            "Decline",
            false,
            ReviewDecision::Denied {
                rejection: "Declined in Office Building.".to_string(),
            },
        ),
        ("Decline and stop the turn", false, ReviewDecision::Abort),
    ]
    .into_iter()
    .map(|(label, allow, decision)| (label.to_string(), allow, ReplyBody::Review { decision }))
    .collect()
}

pub(super) fn legacy_exec(p: LegacyExecParams) -> Ask {
    Ask {
        fields: Vec::new(),
        fill: None,
        title: "Run this command?".to_string(),
        detail: lines([
            Some(p.command.join(" ")),
            Some(format!("in {}", p.cwd)),
            p.reason,
        ]),
        item: None,
        choices: legacy_choices(),
        background: false,
        class: Class::Approval,
    }
}

pub(super) fn legacy_patch(p: LegacyPatchParams) -> Ask {
    let files = p
        .file_changes
        .into_iter()
        .map(|(path, change)| match change {
            LegacyFileChange::Add { content } => format!("Add {path}\n{content}"),
            LegacyFileChange::Delete => format!("Delete {path}"),
            LegacyFileChange::Update { unified_diff } => format!("Edit {path}\n{unified_diff}"),
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ask {
        fields: Vec::new(),
        fill: None,
        title: "Apply these file changes?".to_string(),
        detail: lines([p.reason, Some(files)]),
        item: None,
        choices: legacy_choices(),
        background: false,
        class: Class::Approval,
    }
}

/// A command kind the building does not know (`humanCommand`, or a later one)
/// is offered only accept, decline and cancel, never an answer Codex remembers.
pub(super) fn command_approval(p: CommandApprovalParams, cwd: &str) -> Ask {
    let unknown_kind = p.kind == Some(CommandApprovalKind::Other);
    let decisions = if unknown_kind {
        let plain: Vec<CommandDecision> = p
            .available_decisions
            .unwrap_or_default()
            .into_iter()
            .filter(|d| {
                matches!(
                    d,
                    CommandDecision::Accept | CommandDecision::Decline | CommandDecision::Cancel
                )
            })
            .collect();
        if plain.is_empty() {
            vec![
                CommandDecision::Accept,
                CommandDecision::Decline,
                CommandDecision::Cancel,
            ]
        } else {
            plain
        }
    } else {
        p.available_decisions.unwrap_or_else(|| {
            let mut d = vec![CommandDecision::Accept, CommandDecision::AcceptForSession];
            if let Some(amendment) = p.proposed_execpolicy_amendment {
                d.push(CommandDecision::AcceptWithExecpolicyAmendment {
                    execpolicy_amendment: amendment,
                });
            }
            for amendment in p.proposed_network_policy_amendments.unwrap_or_default() {
                d.push(CommandDecision::ApplyNetworkPolicyAmendment {
                    network_policy_amendment: amendment,
                });
            }
            d.extend([CommandDecision::Decline, CommandDecision::Cancel]);
            d
        })
    };
    let command = command_line(
        p.command_actions.unwrap_or_default(),
        p.command.unwrap_or_default(),
    );
    let title = match (&p.network_approval_context, p.kind) {
        (_, Some(CommandApprovalKind::Other)) => {
            "Codex asks for your decision on a command".to_string()
        }
        (Some(net), _) => format!("Allow network access to {}?", net.host),
        (None, Some(CommandApprovalKind::WriteStdin)) => {
            "Send input to a running command?".to_string()
        }
        (None, _) => "Run this command?".to_string(),
    };
    let cwd = p.cwd.filter(|c| c != cwd).map(|c| format!("in {c}"));
    let detail = lines([Some(command).filter(|c| !c.is_empty()), cwd, p.reason]);
    let choices = decisions
        .into_iter()
        .map(|d| {
            let (label, allow) = command_choice(&d);
            (label, allow, ReplyBody::Decision { decision: d })
        })
        .collect();
    Ask {
        fields: Vec::new(),
        fill: None,
        title,
        detail,
        item: Some(p.item_id),
        choices,
        background: false,
        class: Class::Approval,
    }
}

pub(super) fn file_approval(p: FileApprovalParams, files: Option<String>) -> Ask {
    let grant = p
        .grant_root
        .map(|root| format!("Also lets Codex write anywhere under {root} for this session."));
    let detail = lines([p.reason, grant, files]);
    let choices = [
        CommandDecision::Accept,
        CommandDecision::AcceptForSession,
        CommandDecision::Decline,
        CommandDecision::Cancel,
    ]
    .into_iter()
    .map(|d| {
        let (label, allow) = match d {
            CommandDecision::Accept => ("Apply the change", true),
            CommandDecision::AcceptForSession => ("Allow edits for this session", true),
            CommandDecision::Cancel => ("Decline and stop the turn", false),
            _ => ("Decline", false),
        };
        (
            label.to_string(),
            allow,
            ReplyBody::Decision { decision: d },
        )
    })
    .collect();
    Ask {
        fields: Vec::new(),
        fill: None,
        title: "Apply these file changes?".to_string(),
        detail,
        item: Some(p.item_id),
        choices,
        background: false,
        class: Class::Approval,
    }
}

/// The present lines, joined.
pub(super) fn lines(parts: impl IntoIterator<Item = Option<String>>) -> String {
    parts
        .into_iter()
        .flatten()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// A command as the human would type it: the parsed actions (without the
/// `/bin/zsh -lc` wrapper Codex runs them in), or the raw line.
pub(super) fn command_line(actions: Vec<CommandAction>, command: String) -> String {
    if actions.is_empty() {
        command
    } else {
        actions
            .into_iter()
            .map(|a| a.command)
            .collect::<Vec<_>>()
            .join(" && ")
    }
}
