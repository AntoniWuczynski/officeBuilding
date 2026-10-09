//! Small file operations for the generated launch folders: they only ever add,
//! replace or remove single entries, so an agent already running from a folder
//! keeps what it loaded.

use std::fs;
use std::path::{Path, PathBuf};

/// The entries of `dir`, dotfiles included, as (name, path).
pub(super) fn entries(dir: &Path) -> Vec<(String, PathBuf)> {
    fs::read_dir(dir)
        .map(|it| {
            it.flatten()
                .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
                .collect()
        })
        .unwrap_or_default()
}

fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

/// Make `dir` hold exactly the links in `wanted`; anything that is not a link
/// (a tool's own files) stays.
pub(super) fn sync_links(dir: &Path, wanted: &[(String, PathBuf)]) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    for (name, path) in entries(dir) {
        if is_link(&path) && !wanted.iter().any(|(n, _)| *n == name) {
            remove_file(&path)?;
        }
    }
    for (name, target) in wanted {
        link(&dir.join(name), target)?;
    }
    Ok(())
}

/// Point `at` to `target`, replacing a link that points elsewhere. A real file
/// already at `at` is left alone.
pub(super) fn link(at: &Path, target: &Path) -> Result<(), String> {
    match fs::read_link(at) {
        Ok(current) if current == target => return Ok(()),
        Ok(_) => remove_file(at)?,
        Err(_) if fs::symlink_metadata(at).is_ok() => return Ok(()),
        Err(_) => {}
    }
    std::os::unix::fs::symlink(target, at)
        .map_err(|e| format!("could not link {}: {e}", at.display()))
}

/// At `at`, either the generated `text` or (with none) a link back to `real`,
/// when there is one. Used for the overlay files the building writes itself.
pub(super) fn generated_or_link(at: &Path, text: Option<&str>, real: &Path) -> Result<(), String> {
    if let Some(text) = text {
        if is_link(at) {
            remove_file(at)?;
        }
        return write_file(at, text);
    }
    if fs::symlink_metadata(at).is_ok() && !is_link(at) {
        remove_file(at)?;
    }
    if real.exists() {
        link(at, real)
    } else {
        remove_file(at)
    }
}

/// Write `text` unless the file already holds it (or remove the file with none).
pub(super) fn write_or_remove(path: &Path, text: Option<&str>) -> Result<(), String> {
    match text {
        Some(text) => write_file(path, text),
        None => remove_file(path),
    }
}

pub(super) fn write_file(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }
    if fs::read_to_string(path).ok().as_deref() == Some(text) {
        return Ok(());
    }
    fs::write(path, text).map_err(|e| format!("could not write {}: {e}", path.display()))
}

/// Remove a file or link; one that is not there is already gone.
pub(super) fn remove_file(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("could not remove {}: {e}", path.display())),
    }
}
