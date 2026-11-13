//! Codex rollout lines shared by the reducer's tests and the handover's (test only).

// The payload codex-cli 0.161.0 wrote when a local listener replayed a used-up allowance
// to it through a custom provider on 2026-11-13 (probe rounds six and seven). A replay,
// not a reply from the ChatGPT backend. The line's `timestamp` is composed.
pub(crate) const GOAL: &str = r#"{"timestamp":"2026-11-13T12:43:16.000Z","type":"event_msg","payload":{"type":"thread_goal_updated","threadId":"0199c0de-0000-4000-8000-000000000001","goal":{"threadId":"0199c0de-0000-4000-8000-000000000001","objective":"Reply ok until told to stop","status":"active","tokensUsed":0,"timeUsedSeconds":0,"createdAt":1794573796,"updatedAt":1794573796}}}"#;
// The payload codex-cli 0.161.0 wrote when a local listener replayed a used-up allowance
// to it through a custom provider on 2026-11-13 (probe rounds six and seven). A replay,
// not a reply from the ChatGPT backend. The line's `timestamp` is composed.
pub(crate) const LIMIT: &str = r#"{"timestamp":"2026-11-13T12:43:16.000Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-1","last_agent_message":null,"error":{"message":"You’ve hit your usage limit. Upgrade to Pro (https://chatgpt.com/explore/pro), visit https://chatgpt.com/settings/usage to purchase more credits or try again at 2:41 PM.","codex_error_info":"usage_limit_exceeded"},"started_at":1794573796,"completed_at":1794573796,"duration_ms":10}}"#;
// Composed from the key names of rollouts on one machine and from the Codex 0.161.0
// source, not from a replay. The ids and the two account identifiers are invented: they
// are there so the tests can show they go nowhere.
pub(crate) const META: &str = r#"{"timestamp":"2026-11-13T12:43:15.000Z","type":"session_meta","payload":{"id":"0199c0de-0000-4000-8000-000000000001","session_id":"0199c0de-0000-4000-8000-000000000001","cwd":"/work/app","originator":"codex-tui","cli_version":"0.161.0","source":"cli","model_provider":"openai","history_mode":"paginated","creator_user_id":"user-invented-for-this-test","creator_account_id":"account-invented-for-this-test"}}"#;
// Composed from the key names of rollouts on one machine and from the Codex 0.161.0
// source, not from a replay.
pub(crate) const CLEAN: &str = r#"{"timestamp":"2026-11-13T12:45:00.000Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-2","last_agent_message":"Done.","started_at":1794573880,"completed_at":1794573900,"duration_ms":20}}"#;
// Composed from the key names of rollouts on one machine and from the Codex 0.161.0
// source, not from a replay.
pub(crate) const ABORTED: &str = r#"{"timestamp":"2026-11-13T12:45:00.000Z","type":"event_msg","payload":{"type":"turn_aborted","turn_id":"turn-1","reason":"interrupted"}}"#;
// Composed from the key names of rollouts on one machine and from the Codex 0.161.0
// source, not from a replay.
pub(crate) const ERROR_EVENT: &str = r#"{"timestamp":"2026-11-13T12:45:00.000Z","type":"event_msg","payload":{"type":"error","message":"You've hit your usage limit.","codex_error_info":"usage_limit_exceeded"}}"#;

/// The thread id `META` and `GOAL` carry.
pub(crate) const ID: &str = "0199c0de-0000-4000-8000-000000000001";
