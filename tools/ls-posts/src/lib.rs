//! Reconstruct gallery-dl post URLs from a downloaded folder tree.

pub mod cli;
pub mod error;
pub mod record;
pub mod site;

use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

use walkdir::WalkDir;

use crate::cli::{Cli, OutputMode};
use crate::error::Skip;
use crate::record::Record;
use crate::site::Site;

/// Program name used in diagnostics.
const PROG: &str = "ls-posts";

/// Run the CLI, writing records to `out` and diagnostics to `err`.
///
/// Returns the process exit code: `0` on success, `1` on an I/O failure, or
/// `2` on a usage/`--format` error.
pub fn run<I, S>(args: I, out: &mut impl Write, err: &mut impl Write) -> i32
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut argv = vec![OsString::from(PROG)];
    argv.extend(args.into_iter().map(Into::into));

    let command = match Cli::parse_from(argv) {
        Ok(command) => command,
        Err(error) => {
            let code = error.exit_code();
            let _ = error.print();
            return code;
        }
    };

    let mode = match command.output_mode() {
        Ok(mode) => mode,
        Err(error) => {
            let _ = writeln!(err, "{PROG}: {error}");
            return 2;
        }
    };

    let inputs = if command.paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        command.paths.clone()
    };

    let mut seen = HashSet::new();
    let mut records = Vec::new();
    let mut skipped = 0_usize;

    for path in expand(&inputs, &mut *err) {
        let key = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if !seen.insert(key) {
            continue;
        }
        match record_for(&path) {
            Ok(record) => records.push(record),
            Err(skip) => {
                skipped += 1;
                if command.verbose {
                    let _ = writeln!(err, "{PROG}: skipping {}: {skip}", path.display());
                }
            }
        }
    }

    if !command.no_sort {
        records.sort_by(|a, b| a.path.cmp(&b.path));
    }

    if let Err(error) = emit(&mode, &records, command.null, &mut *out) {
        let _ = writeln!(err, "{PROG}: {error}");
        return 1;
    }

    if skipped > 0 && !command.quiet {
        let _ = writeln!(err, "{PROG}: skipped {skipped} file(s); use -v to see why");
    }

    let _ = out.flush();
    0
}

/// Expand directories into the files beneath them, preserving nothing but set
/// membership. Missing paths are reported to `err` and otherwise ignored.
fn expand(paths: &[PathBuf], err: &mut impl Write) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for raw in paths {
        if raw.is_dir() {
            let walked = WalkDir::new(raw)
                .sort_by_file_name()
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_file())
                .map(walkdir::DirEntry::into_path);
            files.extend(walked);
        } else if raw.exists() {
            files.push(raw.clone());
        } else {
            let _ = writeln!(err, "{PROG}: {}: no such file or directory", raw.display());
        }
    }
    files
}

/// Resolve one discovered file to a record.
fn record_for(path: &Path) -> Result<Record, Skip> {
    let parts: Vec<&str> = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => part.to_str(),
            _ => None,
        })
        .collect();

    let Some((name, dirs)) = parts.split_last() else {
        return Err(Skip::UnknownSite);
    };
    let Some((index, site)) = dirs
        .iter()
        .enumerate()
        .find_map(|(i, part)| Site::from_folder(part).map(|site| (i, site)))
    else {
        return Err(Skip::UnknownSite);
    };

    let resolved = site::parse(site, &dirs[index..], name)?;
    Ok(Record::new(path, name, &resolved))
}

/// Write the records according to `mode`.
fn emit(mode: &OutputMode, records: &[Record], null: bool, out: &mut impl Write) -> io::Result<()> {
    match mode {
        OutputMode::Json => {
            let text = serde_json::to_string_pretty(records).map_err(io::Error::other)?;
            writeln!(out, "{text}")
        }
        OutputMode::Jsonl => {
            for record in records {
                let text = serde_json::to_string(record).map_err(io::Error::other)?;
                writeln!(out, "{text}")?;
            }
            Ok(())
        }
        OutputMode::Format(template) => {
            let separator = if null { "\0" } else { "\n" };
            let rendered: Vec<String> = records.iter().map(|r| template.render(r)).collect();
            out.write_all(rendered.join(separator).as_bytes())?;
            if !rendered.is_empty() {
                out.write_all(separator.as_bytes())?;
            }
            Ok(())
        }
    }
}
