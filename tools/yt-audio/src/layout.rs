//! Output path layout.

use std::path::{Path, PathBuf};

/// Longest sanitized component in bytes: filesystems cap names at 255, and
/// this leaves room for a " (N)" suffix and the extension.
const MAX_COMPONENT_BYTES: usize = 200;

/// Make `name` safe as one path component, portable to FAT/NTFS targets.
pub fn sanitize(name: &str) -> String {
    let replaced: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let mut end = replaced.len().min(MAX_COMPONENT_BYTES);
    while !replaced.is_char_boundary(end) {
        end -= 1;
    }
    let trimmed = replaced[..end].trim_end_matches(['.', ' ']);
    if trimmed.is_empty() {
        "_".into()
    } else {
        trimmed.into()
    }
}

/// Where a track goes: `base/<folder>/<title>`, without extension
/// (yt-dlp adds it).
pub fn destination(base: &Path, folder: &str, title: &str) -> PathBuf {
    base.join(sanitize(folder)).join(sanitize(title))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_slash() {
        assert_eq!(sanitize("AC/DC"), "AC_DC");
    }

    #[test]
    fn sanitize_replaces_portable_reserved() {
        assert_eq!(sanitize("a\\b:c*d?e\"f<g>h|i\0j"), "a_b_c_d_e_f_g_h_i_j");
    }

    #[test]
    fn sanitize_control_chars() {
        assert_eq!(sanitize("a\nb\tc"), "a_b_c");
    }

    #[test]
    fn sanitize_truncates_on_char_boundary() {
        let got = sanitize(&"曲".repeat(100));
        assert_eq!(got.len(), 198);
        assert_eq!(got.chars().count(), 66);
    }

    #[test]
    fn sanitize_empty_becomes_underscore() {
        assert_eq!(sanitize("..."), "_");
        assert_eq!(sanitize(""), "_");
        assert_eq!(sanitize("a. ."), "a");
    }

    #[test]
    fn destination_joins_sanitized() {
        assert_eq!(
            destination(Path::new("/m"), "A/B", "t?"),
            PathBuf::from("/m/A_B/t_")
        );
    }
}
