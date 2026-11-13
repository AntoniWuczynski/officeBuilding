//! `routing-exhaustion.json`: the exhaustion records on disk. Separate from `settings.json`,
//! because that file is reset when it does not parse, and a reset must never reopen an
//! exhausted connection. It holds no content and no credential by construction.

use super::availability::ExhaustionRecord;
use crate::store::write_json_atomic;
use crate::sync::lock;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

const VERSION: u32 = 1;

/// What is written: `{ "version": 1, "records": [ … ] }`.
#[derive(Serialize, Deserialize)]
struct Saved1 {
    version: u32,
    records: Vec<ExhaustionRecord>,
}

/// A copy of the records to write, with the revision it was taken at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub revision: u64,
    pub records: Vec<ExhaustionRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    pub records: Vec<ExhaustionRecord>,
    /// Why the file on disk could not be used.
    pub problem: Option<String>,
}

pub struct ExhaustionFile {
    path: PathBuf,
    /// The revision last written, if any.
    written: Mutex<Option<u64>>,
}

impl ExhaustionFile {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            written: Mutex::new(None),
        }
    }

    /// Never fails. No file: empty. A file that does not read, does not parse, or has another version
    /// is moved aside to `routing-exhaustion.invalid.json` and the records start empty, with `problem` set.
    #[must_use]
    pub fn load(&self) -> Loaded {
        let why = match fs::read_to_string(&self.path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Loaded {
                    records: Vec::new(),
                    problem: None,
                };
            }
            Err(e) => format!("it could not be read: {e}"),
            Ok(text) => match serde_json::from_str::<Saved1>(&text) {
                Ok(saved) if saved.version == VERSION => {
                    return Loaded {
                        records: saved.records,
                        problem: None,
                    };
                }
                Ok(saved) => format!("it has version {}, not {VERSION}", saved.version),
                Err(e) => format!("it is not valid: {e}"),
            },
        };
        let aside = self.path.with_extension("invalid.json");
        let moved = match fs::rename(&self.path, &aside) {
            Ok(()) => format!("it was moved to {}", aside.display()),
            Err(e) => format!("it could not be moved aside: {e}"),
        };
        Loaded {
            records: Vec::new(),
            problem: Some(format!(
                "The routing exhaustion records in {} were not used, because {why}, and {moved}.",
                self.path.display()
            )),
        }
    }

    /// Write `saved`, unless a newer revision was already written. The lock is held across the write,
    /// so two threads cannot leave the older copy on disk.
    pub fn save(&self, saved: &Saved) -> Result<(), String> {
        let mut written = lock(&self.written);
        if written.is_some_and(|last| saved.revision < last) {
            return Ok(());
        }
        write_json_atomic(
            &self.path,
            &Saved1 {
                version: VERSION,
                records: saved.records.clone(),
            },
        )?;
        *written = Some(saved.revision);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::availability::Bucket;
    use crate::routing::config::TierId;
    use crate::routing::fixtures::{conn, temp_path};
    use serde_json::json;
    use std::path::Path;

    fn cleanup(path: &Path) {
        fs::remove_dir_all(path.parent().expect("dir")).expect("cleanup");
    }

    fn record(id: &str, bucket: Bucket) -> ExhaustionRecord {
        ExhaustionRecord {
            connection: conn(id),
            bucket,
            observed_at: 100,
            reset_at: Some(200),
            window: Some("five_hour".to_string()),
            last_probe_at: None,
        }
    }

    fn saved(revision: u64, records: Vec<ExhaustionRecord>) -> Saved {
        Saved { revision, records }
    }

    #[test]
    fn a_save_reads_back_equal() {
        let path = temp_path("routing-records", "round-trip");
        let records = vec![
            record("api", Bucket::Whole),
            record("sub", Bucket::Tier { tier: TierId::Opus }),
            record(
                "sub",
                Bucket::Model {
                    model: "odd-model".to_string(),
                },
            ),
        ];
        ExhaustionFile::new(path.clone())
            .save(&saved(1, records.clone()))
            .expect("save");
        let loaded = ExhaustionFile::new(path.clone()).load();
        assert_eq!(
            loaded,
            Loaded {
                records,
                problem: None
            }
        );
        cleanup(&path);
    }

    #[test]
    fn no_file_loads_empty_with_no_problem() {
        let path = temp_path("routing-records", "missing");
        assert_eq!(
            ExhaustionFile::new(path.clone()).load(),
            Loaded {
                records: Vec::new(),
                problem: None
            }
        );
        cleanup(&path);
    }

    #[test]
    fn a_damaged_file_is_moved_aside_and_loads_empty() {
        let rows = [
            ("not json", "{ this is not json"),
            ("version 2", r#"{ "version": 2, "records": [] }"#),
            ("no version", r#"{ "records": [] }"#),
        ];
        for (name, text) in rows {
            let path = temp_path("routing-records", &name.replace(' ', "-"));
            fs::write(&path, text).expect("write");
            let loaded = ExhaustionFile::new(path.clone()).load();
            assert!(loaded.records.is_empty(), "{name}");
            assert!(loaded.problem.is_some(), "{name}");
            assert!(!path.exists(), "{name}: the damaged file stays in place");
            let aside = path.with_file_name("routing-exhaustion.invalid.json");
            assert_eq!(fs::read_to_string(&aside).expect("aside"), text, "{name}");
            cleanup(&path);
        }
    }

    #[test]
    fn an_older_revision_never_overwrites_a_newer_one() {
        let path = temp_path("routing-records", "older");
        let file = ExhaustionFile::new(path.clone());
        let newer = vec![record("api", Bucket::Whole)];
        file.save(&saved(5, newer.clone())).expect("newer");
        file.save(&saved(4, Vec::new())).expect("older is skipped");
        assert_eq!(file.load().records, newer);
        file.save(&saved(6, Vec::new())).expect("newest");
        assert!(file.load().records.is_empty());
        cleanup(&path);
    }

    #[test]
    fn the_written_text_has_a_version_and_the_records() {
        let path = temp_path("routing-records", "text");
        ExhaustionFile::new(path.clone())
            .save(&saved(1, vec![record("api", Bucket::Whole)]))
            .expect("save");
        let text: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("json");
        assert_eq!(
            text,
            json!({
                "version": 1,
                "records": [{
                    "connection": "api",
                    "bucket": { "kind": "whole" },
                    "observedAt": 100,
                    "resetAt": 200,
                    "window": "five_hour",
                    "lastProbeAt": null
                }]
            })
        );
        cleanup(&path);
    }
}
