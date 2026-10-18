//! Output path layout.

use std::path::{Path, PathBuf};

/// Make `name` safe as one path component, portable to FAT/NTFS targets.
pub fn sanitize(name: &str) -> String {
    let replaced: String = name
        .chars()
        .map(|c| match c {
            '/' | '\0' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect();
    let trimmed = replaced.trim_end_matches(['.', ' ']);
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
