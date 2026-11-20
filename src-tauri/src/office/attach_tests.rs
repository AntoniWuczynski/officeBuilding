use super::attach::attach_command;
use super::exile::stop_agents;
use super::move_tests::live_claude_session;
use super::tests::office;
use super::*;

#[test]
fn a_background_job_is_attached_to_in_the_docked_terminal_and_reused() {
    let (office, dir) = office("attach");
    assert_eq!(
        attach_command("9c4967e5", "/bin/zsh"),
        "claude attach '9c4967e5'"
    );
    assert!(office.terminal_of("s-bg").is_err());
    office
        .attach_in_terminal("s-bg", &dir, "sleep 20")
        .expect("attach");
    let first = office
        .terminal_of("s-bg")
        .expect("the session reaches the attach terminal");
    office
        .attach_in_terminal("s-bg", &dir, "sleep 20")
        .expect("attach again");
    assert_eq!(office.terminal_of("s-bg"), Ok(first));
    assert!(
        office.lock().hired.is_empty(),
        "an attach is not a hire that can fail"
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn detaching_hangs_up_the_attach_and_an_unknown_session_is_a_no_op() {
    let (office, dir) = office("attach-detach");
    office
        .attach_in_terminal("s-bg", &dir, "sleep 20")
        .expect("attach");
    let terminal = office.terminal_of("s-bg").expect("attached");
    let pid = office.terminals.pid(&terminal).expect("pid");
    office.detach_session("s-bg");
    let end = std::time::Instant::now() + Duration::from_secs(10);
    while crate::discovery::claude_code::pid_alive(pid) && std::time::Instant::now() < end {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        !crate::discovery::claude_code::pid_alive(pid),
        "the attach process ended"
    );
    assert!(!office.lock().attached.contains_key("s-bg"));
    office.detach_session("ghost");
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn a_session_without_a_background_job_is_left_to_its_window() {
    let (office, dir) = office("attach-none");
    assert_eq!(
        office.attach_session("ghost"),
        Err("unknown session: ghost".to_string())
    );
    // A live interactive session: its registry entry names no background job.
    let pid = live_claude_session(&dir, "alpha", "s-tty");
    office.refresh();
    assert_eq!(office.attach_session("s-tty"), Ok(false));
    assert!(office.lock().attached.is_empty());
    stop_agents(&[("s-tty".to_string(), pid)], Duration::from_secs(5)).expect("stop");
    std::fs::remove_dir_all(&dir).expect("cleanup");
}
