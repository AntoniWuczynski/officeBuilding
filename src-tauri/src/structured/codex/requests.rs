//! The requests a desk has open, each kept under the thread and turn that own
//! it and its original typed id, and what Codex's own thread status says the
//! desk waits on. The mapper decides when a request opens and ends; this keeps
//! them and builds their replies.

use super::ask::{Ask, Fill, FormKind};
use super::wire::{
    ActiveFlag, Answer, CommandDecision, ElicitAction, FormValue, Reply, ReplyBody, RequestId,
    ReviewDecision, ThreadStatus,
};
use crate::delivery::text;
use crate::structured::{Approval, Choice, Event, Field, FieldKind, Values};

/// Which of Codex's two waiting flags a request counts under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Class {
    /// A command, file-change or permission approval, or an elicitation.
    Approval,
    /// A `request_user_input` question.
    Input,
}

/// A field with the invisible and bidirectional controls taken out of what it shows.
fn seen(mut f: Field) -> Field {
    f.label = text::visible(&f.label);
    f.detail = text::visible(&f.detail);
    if let FieldKind::Pick { options, .. } = &mut f.kind {
        for o in options {
            *o = text::visible(o);
        }
    }
    f
}

impl Class {
    const ALL: [Self; 2] = [Self::Approval, Self::Input];

    fn index(self) -> usize {
        match self {
            Self::Approval => 0,
            Self::Input => 1,
        }
    }

    fn flag(self) -> ActiveFlag {
        match self {
            Self::Approval => ActiveFlag::WaitingOnApproval,
            Self::Input => ActiveFlag::WaitingOnUserInput,
        }
    }

    /// The card shown when the thread status says it waits on this class and
    /// no request the building holds accounts for it.
    fn card(self) -> Approval {
        let (id, title) = match self {
            Self::Approval => (
                "status:approval",
                "Codex says it is waiting for your approval",
            ),
            Self::Input => ("status:input", "Codex says it is waiting for your answer"),
        };
        Approval {
            id: id.to_string(),
            title: title.to_string(),
            detail: "The request itself did not reach the building, so it cannot be answered here. Stop the turn to go on.".to_string(),
            ..Approval::default()
        }
    }
}

/// The id an approval carries for the request `id`: a number as its digits, a string
/// behind `s:`, so `1` and `"1"` are two requests.
pub(super) fn key(id: &RequestId) -> String {
    match id {
        RequestId::Number(n) => n.to_string(),
        RequestId::Text(s) => format!("s:{s}"),
    }
}

/// One request A Human has not settled.
struct Open {
    request: RequestId,
    /// Another thread that asked (a child agent's). None: the desk's own thread, which is
    /// also what a request that names no thread is taken for.
    thread: Option<String>,
    /// The turn it belongs to. None: neither the server nor the mapper could say.
    turn: Option<String>,
    item: Option<String>,
    class: Class,
    /// Per choice (by index), the reply it sends.
    replies: Vec<ReplyBody>,
    fill: Option<Fill>,
    /// Per pick field, its options as the request sent them, before `seen` cleaned them.
    picks: Vec<(String, Vec<String>)>,
    /// What the desk shows for it.
    view: Approval,
}

/// A request that is over, for the mapper to tell the desk.
pub(super) struct Gone {
    pub approval_id: String,
    pub item: Option<String>,
}

impl From<Open> for Gone {
    fn from(open: Open) -> Self {
        Self {
            approval_id: open.view.id,
            item: open.item,
        }
    }
}

/// Every request open on one desk, oldest first.
#[derive(Default)]
pub(super) struct Requests {
    open: Vec<Open>,
}

impl Requests {
    /// Keep a request and return what the desk shows for it. One already open under the
    /// same typed id is replaced in place: the server reuses an id only after it resolved
    /// the first.
    pub(super) fn open(
        &mut self,
        request: &RequestId,
        thread: Option<String>,
        turn: Option<String>,
        ask: Ask,
        asked_by: Option<String>,
    ) -> Approval {
        let (choices, replies): (Vec<Choice>, Vec<ReplyBody>) = ask
            .choices
            .into_iter()
            .enumerate()
            .map(|(i, (label, allow, reply))| {
                (
                    Choice {
                        id: i.to_string(),
                        label: text::visible(&label),
                        allow,
                    },
                    reply,
                )
            })
            .unzip();
        let picks = ask
            .fields
            .iter()
            .filter_map(|f| match &f.kind {
                FieldKind::Pick { options, .. } => Some((f.id.clone(), options.clone())),
                _ => None,
            })
            .collect();
        let view = Approval {
            id: key(request),
            title: text::visible(&ask.title),
            detail: text::visible(&ask.detail),
            fields: ask.fields.into_iter().map(seen).collect(),
            choices,
            background: ask.background,
            failed: None,
            asked_by,
        };
        let open = Open {
            request: request.clone(),
            thread,
            turn,
            item: ask.item,
            class: ask.class,
            replies,
            fill: ask.fill,
            picks,
            view: view.clone(),
        };
        match self.open.iter_mut().find(|o| o.request == *request) {
            Some(kept) => *kept = open,
            None => self.open.push(open),
        }
        view
    }

    /// The line answering `approval_id` with `choice_id`, and whether that choice lets
    /// the agent go ahead. Changes nothing.
    pub(super) fn reply(
        &self,
        approval_id: &str,
        choice_id: &str,
        values: &Values,
    ) -> Result<(String, bool), String> {
        let open = self
            .open
            .iter()
            .find(|o| o.view.id == approval_id)
            .ok_or("that request is no longer open")?;
        if open.replies.is_empty() {
            return Err("that request cannot be answered here".to_string());
        }
        let not_a_choice = || format!("{choice_id} is not one of this request's choices");
        let index: usize = choice_id.parse().map_err(|_| not_a_choice())?;
        let reply = open.replies.get(index).ok_or_else(not_a_choice)?;
        let values = picked(&open.picks, values)?;
        let reply = filled(reply, open.fill.as_ref(), &open.view.fields, &values)?;
        let allow = !matches!(
            reply,
            ReplyBody::Decision {
                decision: CommandDecision::Decline | CommandDecision::Cancel,
            } | ReplyBody::Review {
                decision: ReviewDecision::Denied { .. } | ReviewDecision::Abort,
            } | ReplyBody::Elicit {
                action: ElicitAction::Decline | ElicitAction::Cancel,
                ..
            }
        );
        let line = serde_json::to_string(&Reply {
            id: &open.request,
            result: &reply,
        })
        .map_err(|e| format!("The building could not write a message for Codex: {e}"))?;
        Ok((line, allow))
    }

    /// Its reply was written: the request is over.
    pub(super) fn sent(&mut self, approval_id: &str) -> Option<Gone> {
        let at = self.open.iter().position(|o| o.view.id == approval_id)?;
        Some(self.open.remove(at).into())
    }

    /// Its reply could not be written: it stays open, saying why.
    pub(super) fn failed(&mut self, approval_id: &str, reason: &str) -> Option<Approval> {
        let open = self.open.iter_mut().find(|o| o.view.id == approval_id)?;
        open.view.failed = Some(reason.to_string());
        Some(open.view.clone())
    }

    /// The server resolved the request with exactly this typed id.
    pub(super) fn resolved(&mut self, request: &RequestId) -> Option<Gone> {
        let at = self.open.iter().position(|o| o.request == *request)?;
        Some(self.open.remove(at).into())
    }

    /// A turn of `thread` ended: its requests, and the thread's requests that name no
    /// turn, are over.
    pub(super) fn retire_turn(&mut self, thread: Option<&str>, turn: &str) -> Vec<Gone> {
        self.retire(|o| {
            o.thread.as_deref() == thread && o.turn.as_deref().is_none_or(|t| t == turn)
        })
    }

    /// A turn of `thread` started: every request of that thread from another turn is over.
    pub(super) fn retire_older(&mut self, thread: Option<&str>, turn: &str) -> Vec<Gone> {
        self.retire(|o| o.thread.as_deref() == thread && o.turn.as_deref() != Some(turn))
    }

    fn retire(&mut self, over: impl Fn(&Open) -> bool) -> Vec<Gone> {
        let (gone, kept) = std::mem::take(&mut self.open).into_iter().partition(over);
        self.open = kept;
        gone.into_iter().map(Gone::from).collect::<Vec<_>>()
    }

    /// Some request is open, whether or not the agent can go on without it.
    pub(super) fn any_open(&self) -> bool {
        !self.open.is_empty()
    }

    /// Some open request keeps the agent from going on.
    pub(super) fn blocking(&self) -> bool {
        self.open.iter().any(|o| !o.view.background)
    }

    /// Some open request of the desk's own thread has this class.
    pub(super) fn holds(&self, class: Class) -> bool {
        self.open
            .iter()
            .any(|o| o.thread.is_none() && o.class == class)
    }

    /// Some open request is owned by the other thread `thread`.
    pub(super) fn asking(&self, thread: &str) -> bool {
        self.open
            .iter()
            .any(|o| o.thread.as_deref() == Some(thread))
    }

    /// Some open request names this item.
    pub(super) fn waits_on(&self, item: &str) -> bool {
        self.open.iter().any(|o| o.item.as_deref() == Some(item))
    }
}

/// What Codex's own status last said the desk's thread waits on, per class, and whether
/// a request the building holds accounts for it.
#[derive(Default)]
pub(super) struct Flags {
    /// By `Class`: the flag is set.
    on: [bool; 2],
    /// By `Class`: a request of that class opened, or was open, while it was set.
    explained: [bool; 2],
    /// By `Class`: the card saying so is on the desk.
    shown: [bool; 2],
}

impl Flags {
    /// The desk's thread has this status now.
    pub(super) fn set(&mut self, status: &ThreadStatus, requests: &Requests) {
        let flags: &[ActiveFlag] = match status {
            ThreadStatus::Active { active_flags } => active_flags,
            ThreadStatus::Other => &[],
        };
        for class in Class::ALL {
            let i = class.index();
            let on = flags.contains(&class.flag());
            if on && !self.on[i] {
                self.explained[i] = requests.holds(class);
            } else if !on {
                self.explained[i] = false;
            }
            self.on[i] = on;
        }
    }

    /// A request of the desk's own thread opened.
    pub(super) fn explain(&mut self, class: Class) {
        let i = class.index();
        if self.on[i] {
            self.explained[i] = true;
        }
    }

    /// The desk's turn ended: Codex zeroes its counters.
    pub(super) fn clear(&mut self) {
        self.on = [false; 2];
        self.explained = [false; 2];
    }

    /// The status says the thread waits on something no request accounts for.
    pub(super) fn unseen(&self) -> bool {
        Class::ALL.iter().any(|c| self.unseen_for(*c))
    }

    fn unseen_for(&self, class: Class) -> bool {
        let i = class.index();
        self.on[i] && !self.explained[i]
    }

    /// The cards to show or take away now.
    pub(super) fn cards(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        for class in Class::ALL {
            let i = class.index();
            let unseen = self.unseen_for(class);
            if unseen && !self.shown[i] {
                self.shown[i] = true;
                out.push(Event::Approval(class.card()));
            } else if self.shown[i] && !unseen {
                self.shown[i] = false;
                out.push(Event::Resolved {
                    approval_id: class.card().id,
                });
            }
        }
        out
    }
}

/// The values with each picked option put back as the request sent it, since the desk
/// shows options cleaned of invisible characters and the server matches the original.
/// A pick that two original options both clean to is refused rather than guessed.
fn picked(picks: &[(String, Vec<String>)], values: &Values) -> Result<Values, String> {
    let mut out = values.clone();
    for (id, options) in picks {
        let Some(v) = values.get(id) else { continue };
        let mut hits = options.iter().filter(|o| text::visible(o) == v.trim());
        match (hits.next(), hits.next()) {
            (Some(raw), None) => {
                out.insert(id.clone(), raw.clone());
            }
            (Some(_), Some(_)) => {
                return Err(format!(
                    "{id} matches more than one option, answer it in the terminal"
                ));
            }
            (None, _) => {}
        }
    }
    Ok(out)
}

/// The reply an answer sends: as prepared, or, for a choice that accepts a
/// question or a form, built from the human's values. A required field left
/// empty is refused before anything is sent.
fn filled(
    reply: &ReplyBody,
    fill: Option<&Fill>,
    fields: &[Field],
    values: &Values,
) -> Result<ReplyBody, String> {
    let accepting = matches!(
        reply,
        ReplyBody::Input { .. }
            | ReplyBody::Elicit {
                action: ElicitAction::Accept,
                ..
            }
    );
    let Some(fill) = fill.filter(|_| accepting) else {
        return Ok(reply.clone());
    };
    let value = |id: &str| values.get(id).map(|v| v.trim()).filter(|v| !v.is_empty());
    if let Some(missing) = fields.iter().find(|f| f.required && value(&f.id).is_none()) {
        return Err(format!("{} needs an answer first", missing.label));
    }
    Ok(match fill {
        Fill::Input(ids) => ReplyBody::Input {
            answers: ids
                .iter()
                .filter_map(|id| {
                    let answer = Answer {
                        answers: vec![value(id)?.to_string()],
                    };
                    Some((id.clone(), answer))
                })
                .collect(),
        },
        Fill::Form(kinds) => {
            let mut content = std::collections::BTreeMap::new();
            for (id, kind) in kinds {
                let Some(v) = value(id) else { continue };
                let typed = match kind {
                    FormKind::Text => FormValue::Text(v.to_string()),
                    FormKind::Boolean => FormValue::Bool(match v {
                        "true" => true,
                        "false" => false,
                        _ => return Err(format!("{id} takes yes or no")),
                    }),
                    FormKind::Integer => FormValue::Number(
                        v.parse::<i64>()
                            .map_err(|_| format!("{id} takes a whole number"))?
                            .into(),
                    ),
                    FormKind::Number => FormValue::Number(
                        v.parse::<f64>()
                            .ok()
                            .and_then(serde_json::Number::from_f64)
                            .ok_or_else(|| format!("{id} takes a number"))?,
                    ),
                };
                content.insert(id.clone(), typed);
            }
            ReplyBody::Elicit {
                action: ElicitAction::Accept,
                content: Some(content),
                meta: None,
            }
        }
    })
}
