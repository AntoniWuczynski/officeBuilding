//! Print what discovery sees on this Mac, one line per floor and desk.
//! `cargo run --example scan` (add `--json` for the full snapshot).

use office_building_lib::discovery::Discovery;
use std::time::SystemTime;

/// "-" when the file is absent, else how many entries it holds.
fn count<T>(v: Option<&Vec<T>>) -> String {
    v.as_ref().map_or("-".to_string(), |v| v.len().to_string())
}

fn main() {
    let Some(home) = dirs::home_dir() else {
        eprintln!("the home folder could not be found");
        std::process::exit(1);
    };
    let found = Discovery::new(home).scan(SystemTime::now());
    if std::env::args().any(|a| a == "--json") {
        match serde_json::to_string_pretty(&found.sessions) {
            Ok(json) => println!("{json}"),
            Err(e) => eprintln!("{e}"),
        }
        return;
    }
    for p in &found.projects {
        println!("{} ({})", p.name, p.path);
        for s in found.sessions.iter().filter(|s| s.project_id == p.id) {
            println!(
                "  {:?} {:?} {:?} {} | {} {} | {} tokens, ${:.2}, {} unpriced",
                s.tool,
                s.state,
                s.control,
                s.title,
                s.model.as_deref().unwrap_or("?"),
                s.effort.as_deref().unwrap_or("?"),
                s.spend.tokens,
                s.spend.usd,
                s.spend.unpriced_tokens
            );
        }
    }
    for (id, root) in &found.roots {
        let files = office_building_lib::project_files::read(root, id);
        println!(
            "FILES {}: {} open calls, {} settled, human TODO {}, TODO {}",
            root.display(),
            files.open.len(),
            count(files.decided.as_ref()),
            count(files.human_todos.as_ref()),
            count(files.agent_todos.as_ref())
        );
        for q in &files.open {
            println!("  OPEN {} [{} options]", q.prompt, q.options.len());
        }
    }
    for q in &found.questions {
        println!(
            "QUESTION {} [{} options]: {}",
            q.session_id,
            q.options.len(),
            q.prompt
        );
    }
}
