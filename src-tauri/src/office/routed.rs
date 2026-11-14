//! The office side of provider routing: which launch is routed, the routed launch itself, and
//! the route of a desk for the live route calls.
//!
//! Routed: Claude Code hires and resumes through `spawn_in_terminal` and `resume_session` only.
//! The QA reviewer, the manager, structured desks and every other tool launch as before. The
//! office lock and the router's locks are never held together.

use super::{Launched, Office};
use crate::model::ToolKind;
use crate::pty;
use crate::routing::config::ConnectionId;
use crate::routing::launch;
use crate::routing::live::DeskRoute;
use crate::routing::router::{NOT_IN_ORDER, NOT_ROUTED, Opened, Router};
use crate::routing::state::RouteId;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

impl Office {
    /// Route this office's Claude Code hires and resumes through `router`.
    #[must_use]
    pub fn with_routing(mut self, router: Arc<Router>) -> Self {
        self.routing = Some(router);
        self
    }

    /// `command` as this launch runs it, and the route it opened if the launch is routed.
    /// Err: the launch should be routed and the router could not take it (E16), so it is not
    /// started at all.
    pub(super) fn routed(
        &self,
        project_id: &str,
        tool: ToolKind,
        resumed: bool,
        command: String,
        shell: &str,
    ) -> Result<(String, Option<Opened>), String> {
        let (Some(router), Some(settings)) = (&self.routing, &self.settings) else {
            return Ok((command, None));
        };
        let routing = settings.get().routing;
        if tool != ToolKind::ClaudeCode || !routing.enabled {
            return Ok((command, None));
        }
        // An unknown floor is not routed (D4).
        let Ok(Some(context)) = self.floor_context(project_id) else {
            return Ok((command, None));
        };
        match router.open(&routing, &context, resumed) {
            Ok(None) => Ok((command, None)),
            Ok(Some(opened)) => {
                let quote = |s: &str| pty::quote(shell, s);
                Ok((
                    launch::prefix(&opened.base_url, &quote) + &command,
                    Some(opened),
                ))
            }
            Err(why) => Err(format!(
                "routing is on for this floor and the building's router could not take the desk, so it was not started: {why}"
            )),
        }
    }

    /// `launch`, or for a routed launch `launch_with` carrying the secret line and the proxy
    /// exclusion in the terminal's environment, then the route bound to its terminal. A launch
    /// that fails closes its route.
    pub(super) fn launch_routed(
        &self,
        dir: &Path,
        command: &str,
        launched: Launched,
        route: Option<Opened>,
    ) -> Result<String, String> {
        let (Some(opened), Some(router)) = (route, &self.routing) else {
            return self.launch(dir, command, launched);
        };
        let inherited = std::env::var("NO_PROXY")
            .or_else(|_| std::env::var("no_proxy"))
            .ok();
        let np = launch::no_proxy(inherited.as_deref());
        let env = [
            (launch::CUSTOM_HEADERS, opened.header.expose()),
            ("NO_PROXY", np.as_str()),
            ("no_proxy", np.as_str()),
        ];
        let terminal = match self.launch_with(dir, command, launched, &env) {
            Ok(terminal) => terminal,
            Err(why) => {
                router.close(&opened.id);
                return Err(why);
            }
        };
        router.bind(&opened.id, &terminal);
        // The terminal may have exited before the bind, and its exit event gone by unheard.
        if self.terminals.wait_exit(&terminal, Duration::ZERO) != Ok(None) {
            router.close(&opened.id);
        }
        Ok(terminal)
    }

    /// The router and the route of the desk `session_id`, if it is routed. A session after
    /// `/clear` keeps its terminal, so it keeps its route.
    fn route_of_desk(&self, session_id: &str) -> Option<(&Arc<Router>, RouteId)> {
        let router = self.routing.as_ref()?;
        let terminal = self.terminal_of(session_id).ok()?;
        Some((router, router.route_of(&terminal)?))
    }

    /// The route of the desk `session_id`, if it is routed. May read the Keychain.
    #[must_use]
    pub fn desk_route(&self, session_id: &str) -> Option<DeskRoute> {
        let (router, id) = self.route_of_desk(session_id)?;
        router.view(&id)
    }

    /// Arm one check of an exhausted connection in this desk's own order.
    ///
    /// # Errors
    /// The desk is not routed, the connection is not in its order, or it has no record.
    pub fn retry_route(&self, session_id: &str, connection_id: &str) -> Result<DeskRoute, String> {
        let (router, id, connection) = self.route_call(session_id, connection_id)?;
        router.retry(&id, &connection)
    }

    /// Move this desk to another entry of its own order.
    ///
    /// # Errors
    /// The desk is not routed, the connection is not in its order, or it cannot be used now.
    pub fn switch_route(&self, session_id: &str, connection_id: &str) -> Result<DeskRoute, String> {
        let (router, id, connection) = self.route_call(session_id, connection_id)?;
        router.switch(&id, &connection)
    }

    /// The router, the desk's route and the named connection for a retry or a switch.
    fn route_call(
        &self,
        session_id: &str,
        connection_id: &str,
    ) -> Result<(&Arc<Router>, RouteId, ConnectionId), String> {
        let (router, id) = self.route_of_desk(session_id).ok_or(NOT_ROUTED)?;
        let connection = ConnectionId::parse(connection_id).ok_or(NOT_IN_ORDER)?;
        Ok((router, id, connection))
    }
}

#[cfg(test)]
mod tests {
    use super::super::launch::{Desk, desk_session_id};
    use super::super::tests::floor_office;
    use super::*;
    use crate::model::Project;
    use crate::routing::fixtures::secret_of;
    use crate::routing::fixtures::{conn, ctx, settings};
    use crate::routing::keys::Keys;
    use crate::routing::records::ExhaustionFile;
    use crate::routing::upstream::{Bases, FakeUpstream, SendError, Upstream};
    use crate::secrets::MemorySecrets;
    use crate::settings::{RoutingSettings, SettingsPatch, SettingsStore};
    use std::path::PathBuf;
    use std::time::Instant;

    const COMMAND: &str = "claude --model 'claude-opus-5-5' -- 'a task'";
    const SHELL: &str = "/bin/zsh";

    struct Rig {
        office: Office,
        dir: PathBuf,
        floor: Project,
        router: Arc<Router>,
    }

    fn router(dir: &Path) -> Arc<Router> {
        let fake: Arc<dyn Upstream> = Arc::new(FakeUpstream::new(|_| Err(SendError::NotSent)));
        Router::new(
            ExhaustionFile::new(dir.join("routing-exhaustion.json")),
            Arc::new(Keys::new(Arc::new(MemorySecrets::new()))),
            fake,
            Bases::default(),
        )
    }

    /// An office on a floor with `routing` in its settings, the floor given `context`, and a
    /// router unless `routed` is false.
    fn rig(tag: &str, routing: RoutingSettings, context: Option<&str>, with_router: bool) -> Rig {
        let (office, dir, floor) = floor_office(&format!("routed-{tag}"));
        let store = Arc::new(SettingsStore::open(dir.join("settings.json")));
        store
            .update(SettingsPatch {
                routing: Some(routing),
                ..SettingsPatch::default()
            })
            .expect("settings saved");
        let router = router(&dir);
        let mut office = office.with_settings(store);
        if with_router {
            office = office.with_routing(Arc::clone(&router));
        }
        office
            .set_floor_context(&floor.id, context.map(ctx))
            .expect("context set");
        Rig {
            office,
            dir,
            floor,
            router,
        }
    }

    fn routed_rig(tag: &str) -> Rig {
        rig(tag, settings(), Some("work"), true)
    }

    impl Rig {
        fn routed(
            &self,
            tool: ToolKind,
            command: &str,
        ) -> Result<(String, Option<Opened>), String> {
            self.office
                .routed(&self.floor.id, tool, false, command.to_owned(), SHELL)
        }

        fn desk(&self) -> Desk {
            Desk {
                tool: ToolKind::ClaudeCode,
                project_id: self.floor.id.clone(),
                root: self.dir.join("home/code/app"),
                title: "write docs".to_owned(),
                model: "claude-opus-5-5".to_owned(),
                effort: "high".to_owned(),
            }
        }

        fn cleanup(self) {
            std::fs::remove_dir_all(&self.dir).expect("cleanup");
        }
    }

    #[test]
    fn a_launch_that_is_not_routed_keeps_its_command_byte_for_byte() {
        let off = RoutingSettings {
            enabled: false,
            ..settings()
        };
        let mut empty = settings();
        for context in &mut empty.contexts {
            context.chain.clear();
        }
        let rows = [
            (
                "a Codex hire",
                ToolKind::Codex,
                settings(),
                Some("work"),
                true,
            ),
            ("routing off", ToolKind::ClaudeCode, off, Some("work"), true),
            (
                "a floor with no context",
                ToolKind::ClaudeCode,
                settings(),
                None,
                true,
            ),
            (
                "a context that has left Settings",
                ToolKind::ClaudeCode,
                settings(),
                Some("gone"),
                true,
            ),
            (
                "an empty order",
                ToolKind::ClaudeCode,
                empty,
                Some("work"),
                true,
            ),
            (
                "an office with no router",
                ToolKind::ClaudeCode,
                settings(),
                Some("work"),
                false,
            ),
        ];
        for (index, (name, tool, routing, context, with_router)) in rows.into_iter().enumerate() {
            let rig = rig(&format!("plain-{index}"), routing, context, with_router);
            assert_eq!(
                rig.routed(tool, COMMAND),
                Ok((COMMAND.to_owned(), None)),
                "{name}"
            );
            rig.cleanup();
        }
    }

    #[test]
    fn a_routed_launch_gets_the_prefix_and_an_open_route_and_no_secret_on_its_command() {
        let rig = routed_rig("prefix");
        let (command, opened) = rig.routed(ToolKind::ClaudeCode, COMMAND).expect("routed");
        let opened = opened.expect("a route");
        let quote = |s: &str| pty::quote(SHELL, s);
        assert_eq!(
            command,
            format!("{}{COMMAND}", launch::prefix(&opened.base_url, &quote))
        );
        assert!(command.starts_with("env -u ANTHROPIC_API_KEY "));
        assert!(!command.contains(&secret_of(&opened)));
        assert!(rig.router.view(&opened.id).is_some());
        rig.router.close(&opened.id);
        rig.cleanup();
    }

    /// The backlog's plain text once it holds `marker`, waiting up to a few seconds.
    fn printed(office: &Office, terminal: &str, marker: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let text = pty::plain_text(&office.terminals.backlog(terminal).expect("backlog").data);
            if text.contains(marker) || Instant::now() >= deadline {
                return text;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn a_routed_terminal_carries_the_secret_in_its_environment_only_and_its_exit_closes_the_route()
    {
        let rig = routed_rig("launch");
        let script = "/bin/sh -c '/usr/bin/env; echo hired; exec /bin/cat'";
        let shell = pty::login_shell();
        let (command, opened) = rig
            .office
            .routed(
                &rig.floor.id,
                ToolKind::ClaudeCode,
                false,
                script.to_owned(),
                &shell,
            )
            .expect("routed");
        let opened = opened.expect("a route");
        let secret = secret_of(&opened);
        let terminal = rig
            .office
            .launch_routed(
                &rig.dir.join("home/code/app"),
                &command,
                Launched::Desk(rig.desk()),
                Some(opened.clone()),
            )
            .expect("launched");
        let env = printed(&rig.office, &terminal, "hired");
        for line in [
            format!("ANTHROPIC_CUSTOM_HEADERS=x-office-building-route: {secret}"),
            format!("ANTHROPIC_BASE_URL={}", opened.base_url),
            "CLAUDE_CODE_GATEWAY_HINT_HEADERS=1".to_owned(),
            "NO_PROXY=".to_owned(),
            "no_proxy=".to_owned(),
        ] {
            assert!(env.contains(&line), "{line} in the terminal's environment");
        }
        for name in launch::UNSET {
            assert!(!env.contains(&format!("\n{name}=")), "{name} removed");
        }
        let ps = std::process::Command::new("/bin/ps")
            .args(["-axww", "-o", "command="])
            .output()
            .expect("ps");
        assert!(ps.status.success());
        assert!(!String::from_utf8_lossy(&ps.stdout).contains(&secret));
        assert_eq!(rig.router.route_of(&terminal), Some(opened.id.clone()));
        let session = desk_session_id(&terminal);
        let route = rig.office.desk_route(&session).expect("routed desk");
        assert_eq!(route.requests, 0);
        assert_eq!(
            rig.office.switch_route(&session, "Not An Id"),
            Err(NOT_IN_ORDER.to_owned())
        );
        assert_eq!(
            rig.office.retry_route(&session, "Not An Id"),
            Err(NOT_IN_ORDER.to_owned())
        );
        assert_eq!(
            rig.office.retry_route(&session, conn("api").as_str()),
            Err(
                "that connection has no used-up allowance on record, so there is nothing to retry"
                    .to_owned()
            )
        );
        rig.office.terminal_write(&session, "\u{4}").expect("eot");
        rig.office
            .terminals
            .wait_exit(&terminal, Duration::from_secs(10))
            .expect("wait")
            .expect("exited");
        rig.router.terminal_exited(&terminal);
        assert_eq!(rig.router.route_of(&terminal), None);
        assert_eq!(rig.router.view(&opened.id), None);
        assert_eq!(rig.office.desk_route(&session), None);
        rig.cleanup();
    }

    #[test]
    fn a_launch_that_fails_at_once_leaves_no_route_behind() {
        let rig = routed_rig("fails");
        let (command, opened) = rig
            .office
            .routed(
                &rig.floor.id,
                ToolKind::ClaudeCode,
                false,
                "/usr/bin/false".to_owned(),
                &pty::login_shell(),
            )
            .expect("routed");
        let opened = opened.expect("a route");
        assert!(
            rig.office
                .launch_routed(
                    &rig.dir.join("home/code/app"),
                    &command,
                    Launched::Desk(rig.desk()),
                    Some(opened.clone()),
                )
                .is_err()
        );
        assert_eq!(rig.router.view(&opened.id), None);
        rig.cleanup();
    }

    #[test]
    fn a_desk_that_is_not_routed_has_no_route_to_read_retry_or_switch() {
        let rig = routed_rig("unrouted");
        let desk = Desk {
            tool: ToolKind::Antigravity,
            ..rig.desk()
        };
        let terminal = rig
            .office
            .launch(&desk.root.clone(), "/bin/cat", Launched::Desk(desk))
            .expect("launch");
        for session in [desk_session_id(&terminal), "no-such-desk".to_owned()] {
            assert_eq!(rig.office.desk_route(&session), None, "{session}");
            assert_eq!(
                rig.office.retry_route(&session, "api"),
                Err(NOT_ROUTED.to_owned()),
                "{session}"
            );
            assert_eq!(
                rig.office.switch_route(&session, "api"),
                Err(NOT_ROUTED.to_owned()),
                "{session}"
            );
        }
        rig.office
            .terminal_write(&desk_session_id(&terminal), "\u{4}")
            .expect("eot");
        rig.office
            .terminals
            .wait_exit(&terminal, Duration::from_secs(10))
            .expect("wait")
            .expect("exited");
        rig.cleanup();
    }

    #[test]
    fn the_role_launches_stay_unrouted() {
        let sources = [
            ("reviewers.rs", include_str!("reviewers.rs")),
            ("manager.rs", include_str!("manager.rs")),
            ("staff.rs", include_str!("staff.rs")),
        ];
        for (name, source) in sources {
            assert!(!source.contains("launch_routed"), "{name}");
            assert!(!source.contains("routed("), "{name}");
        }
    }
}
