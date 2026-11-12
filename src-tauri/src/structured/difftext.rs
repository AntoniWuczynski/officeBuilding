//! Diffs as plain text: the lines of a file change, and a change's text for an approval.

use super::FileDiff;

/// The changed middle of a file: lines shared at the start and the end are left
/// out, then every remaining old line with `-` in front, then every remaining
/// new line with `+`. Not a line-by-line diff.
#[must_use]
pub fn changed(old: &str, new: &str) -> String {
    let old: Vec<&str> = old.lines().collect();
    let new: Vec<&str> = new.lines().collect();
    let head = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let tail = old[head..]
        .iter()
        .rev()
        .zip(new[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let removed = old[head..old.len() - tail].iter().map(|l| format!("-{l}"));
    let added = new[head..new.len() - tail].iter().map(|l| format!("+{l}"));
    removed.chain(added).collect::<Vec<_>>().join("\n")
}

/// Each diff as its path on one line and then its text, one after the other.
#[must_use]
pub fn joined(diffs: &[FileDiff]) -> String {
    diffs
        .iter()
        .map(|d| format!("{}\n{}", d.path, d.text))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changed_leaves_out_the_shared_first_and_last_lines() {
        assert_eq!(changed("a\nb\nc\nd", "a\nB\nX\nd"), "-b\n-c\n+B\n+X");
    }

    #[test]
    fn changed_handles_an_empty_side_and_identical_texts() {
        assert_eq!(changed("", "x\ny"), "+x\n+y");
        assert_eq!(changed("x\ny", ""), "-x\n-y");
        assert_eq!(changed("same\ntext", "same\ntext"), "");
    }

    #[test]
    fn joined_puts_each_path_on_its_own_line_before_its_text() {
        let diff = |path: &str, text: &str| FileDiff {
            path: path.to_string(),
            text: text.to_string(),
        };
        assert_eq!(
            joined(&[diff("a.rs", "-x\n+y"), diff("b.rs", "+z")]),
            "a.rs\n-x\n+y\nb.rs\n+z"
        );
        assert_eq!(joined(&[]), "");
    }
}
