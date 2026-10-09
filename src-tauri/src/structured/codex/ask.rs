//! Codex's approval requests turned into what the human sees: a title, a
//! detail and choices, each choice carrying the reply it sends back. Choices
//! follow what each request type allows in the protocol (see `wire.rs`).

use super::wire::{
    CommandAction, CommandApprovalKind, CommandApprovalParams, CommandDecision, ElicitAction,
    Elicitation, ElicitationHead, FileApprovalParams, FileSystemPermissions, FormSchema,
    GrantScope, LegacyExecParams, LegacyFileChange, LegacyPatchParams, NetworkPermissions,
    Permissions, PermissionsParams, ReplyBody, ReviewDecision, RuleAction, UserInputParams,
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

pub(super) fn permissions_approval(p: PermissionsParams) -> Ask {
    let network = p
        .permissions
        .network
        .as_deref()
        .and_then(|n| serde_json::from_str::<NetworkPermissions>(n.get()).ok())
        .and_then(|n| n.enabled)
        .map(|on| {
            if on {
                "Network access"
            } else {
                "No network access"
            }
            .to_string()
        });
    let fs = p
        .permissions
        .file_system
        .as_deref()
        .and_then(|f| serde_json::from_str::<FileSystemPermissions>(f.get()).ok());
    let (read, write) = fs.map_or((None, None), |f| {
        let list = |label: &str, paths: Option<Vec<String>>| {
            paths
                .filter(|p| !p.is_empty())
                .map(|p| format!("{label}: {}", p.join(", ")))
        };
        (list("Read", f.read), list("Write", f.write))
    });
    let detail = lines([p.reason, network, read, write]);
    let grant = |scope| ReplyBody::Permissions {
        permissions: p.permissions.clone(),
        scope,
    };
    let none = ReplyBody::Permissions {
        permissions: Permissions {
            network: None,
            file_system: None,
        },
        scope: GrantScope::Turn,
    };
    Ask {
        fields: Vec::new(),
        fill: None,
        title: "Give Codex extra permissions?".to_string(),
        detail,
        item: Some(p.item_id),
        choices: vec![
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
            ("Decline".to_string(), false, none),
        ],
    }
}

/// Codex's questions become fields (a pick-list for a question with options,
/// text otherwise), answered together with "Send".
pub(super) fn user_input(p: UserInputParams) -> Option<Ask> {
    if p.questions.is_empty() {
        return None;
    }
    let title = match p.questions.as_slice() {
        [q] => lines([
            Some(q.header.clone()).filter(|h| !h.is_empty()),
            Some(q.text.clone()),
        ]),
        _ => "Codex has some questions".to_string(),
    };
    let detail = lines(p.questions.iter().flat_map(|q| {
        q.options.iter().flatten().map(|o| {
            Some(o.description.clone())
                .filter(|d| !d.is_empty())
                .map(|d| format!("{}: {d}", o.label))
        })
    }));
    let ids: Vec<String> = p.questions.iter().map(|q| q.id.clone()).collect();
    let fields = p
        .questions
        .into_iter()
        .map(|q| {
            let kind = match q.options.filter(|o| !o.is_empty()) {
                Some(options) => FieldKind::Pick {
                    options: options.into_iter().map(|o| o.label).collect(),
                    other: q.is_other,
                },
                None if q.is_secret => FieldKind::Secret,
                None => FieldKind::Text,
            };
            Field {
                id: q.id,
                label: if q.header.is_empty() {
                    q.text
                } else {
                    q.header
                },
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
        detail,
        item: None,
        fields,
        fill: Some(Fill::Input(ids)),
        choices: vec![("Send".to_string(), true, send)],
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
    }
}

pub(super) fn command_approval(p: CommandApprovalParams, cwd: &str) -> Ask {
    let decisions = p.available_decisions.unwrap_or_else(|| {
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
    });
    let command = command_line(
        p.command_actions.unwrap_or_default(),
        p.command.unwrap_or_default(),
    );
    let title = match (&p.network_approval_context, p.kind) {
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
