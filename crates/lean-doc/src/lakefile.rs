//! Where `--lib` comes from: the package's own lakefile.
//!
//! Milestone **M4-d**, and M3-d2's fifth debt (plan §7). `lean-doc modules
//! --root <repo> --lib <Name>` made the caller name the library, with "which
//! libraries a package declares is in its lakefile, and reading one is M4's" as
//! the reason. This is that reading.
//!
//! # What is read, and what is refused by name 【判断】
//!
//! A Lake package declares its libraries in one of two files, and they are not
//! the same kind of thing:
//!
//! * **`lakefile.toml`** — data. `[[lean_lib]]` blocks with a `name` key. That
//!   is what the measurement target has, and it is what this reads.
//! * **`lakefile.lean`** — *code*. `lean_lib` is a Lake DSL command in a Lean
//!   file that Lake elaborates; the library name can come from an `open`ed
//!   namespace, from string interpolation, from an `if`. Reading it honestly
//!   means running Lake, which this command does not do. **It is refused by
//!   name, with `--lib` as the answer** — never parsed hopefully.
//!
//! The same rule runs one level deeper, inside the TOML. This is **not** a TOML
//! parser and does not pretend to be one: it is a recogniser for the exact
//! shape the format's `[[lean_lib]]`/`name` pair is written in, and **every line
//! it cannot account for stops it** rather than being skipped. The failure it is
//! built against is silent under-reading — a `[[lean_lib]]` written in a spelling
//! the recogniser skips produces a *shorter* module list, and a module list that
//! is missing a library looks exactly like a package whose modules were deleted:
//! the pages are simply never written and nothing says so.
//!
//! So the refusals below are deliberately loud, and all of them end in the same
//! sentence — pass `--lib`. A caller with a lakefile this cannot read is never
//! stuck; it is only told to say the name out loud.
//!
//! # `defaultTargets` is not consulted 【判断】
//!
//! Every `[[lean_lib]]` is returned, not the subset `defaultTargets` names. The
//! two answer different questions: `defaultTargets` is what `lake build` builds
//! with no arguments (and it can name executables, which have no modules to
//! document), while this list is "which module roots does this package own". A
//! package that declares a library it does not build by default has no olean for
//! it, and `detect` says so by name (exit 3) — which is a better failure than
//! documenting less than the package has and reporting success.

use std::fs;
use std::path::{Path, PathBuf};

use crate::Failure;

/// The libraries a package declares, and the file that said so.
pub struct Libraries {
    pub names: Vec<String>,
    /// For the log line: a caller that gets a surprising module list needs to
    /// know which file the surprise came from.
    pub file: PathBuf,
}

/// The libraries declared in `<root>`'s lakefile.
///
/// Exit 3 (`Failure::Refused`) rather than exit 2 for every failure here: the
/// command line was fine and the *package* is a shape this cannot read, which is
/// the same kind of answer as "this module has no olean".
pub fn read_libraries(root: &Path) -> Result<Libraries, Failure> {
    let toml = root.join("lakefile.toml");
    let lean = root.join("lakefile.lean");
    if !toml.is_file() {
        if lean.is_file() {
            return refuse(format!(
                "{} is Lean code, not data: `lean_lib` there is a Lake DSL command whose argument \
                 can come from an `open`ed namespace or from any Lean expression, so reading it \
                 honestly means elaborating it with Lake — which this command does not do. Pass \
                 --lib <Name> (repeatable) and the glob will use it",
                lean.display(),
            ));
        }
        return refuse(format!(
            "no lakefile.toml and no lakefile.lean under {}: --root names a Lake package, and the \
             library names come from its lakefile. Pass --lib <Name> to name them yourself",
            root.display(),
        ));
    }
    let text = fs::read_to_string(&toml)
        .map_err(|source| Failure::Failed(format!("{}: {source}", toml.display())))?;
    let names = lean_libs(&text, &toml)?;
    Ok(Libraries { names, file: toml })
}

/// Every `[[lean_lib]]`'s `name`, in the order the file declares them.
///
/// The recogniser, stated as rules so that what it does **not** understand is
/// visible:
///
/// 1. a line whose first non-blank character is `[` is a table header; the only
///    one that opens a library block is exactly `[[lean_lib]]`;
/// 2. inside such a block, a line whose key is `name` must be
///    `name = "<Ident>"`, with an optional `#` comment after it, and the string
///    must be a plain one — no escapes;
/// 3. any other line is a key of some table this does not care about, and is
///    skipped **only because rules 1 and 2 make a missed `[[lean_lib]]`
///    impossible to reach**: a header that mentions `lean_lib` in any other
///    spelling stops the run instead of being skipped as "some other table";
/// 4. multi-line strings (`"""` / `'''`) stop the run before any of the above,
///    because their content can be an arbitrary line and rule 1 would read it as
///    structure.
fn lean_libs(text: &str, path: &Path) -> Result<Vec<String>, Failure> {
    if text.contains("\"\"\"") || text.contains("'''") {
        return refuse(format!(
            "{}: multi-line strings are not read — inside one, a line can be anything, and this \
             recogniser reads a leading `[` as a table header. Pass --lib <Name>",
            path.display(),
        ));
    }
    let mut names: Vec<String> = Vec::new();
    let mut open: Option<Option<String>> = None;
    for (number, raw) in text.lines().enumerate() {
        let number = number + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            close(&mut open, &mut names, path, number)?;
            if line == "[[lean_lib]]" {
                open = Some(None);
                continue;
            }
            // Rule 3's guard. `[ [lean_lib] ]` and `[[lean_lib.extra]]` are
            // valid TOML that this does not understand; treating either as "some
            // other table" would drop a library without a word.
            if line.contains("lean_lib") {
                return refuse(format!(
                    "{path}:{number}: `{line}` mentions lean_lib in a spelling this does not read \
                     (only a bare `[[lean_lib]]` header is). Skipping it would document fewer \
                     libraries than the package has, silently. Pass --lib <Name>",
                    path = path.display(),
                ));
            }
            continue;
        }
        let Some(slot) = open.as_mut() else {
            continue;
        };
        let Some(rest) = key_is_name(line) else {
            continue;
        };
        let Some(name) = plain_string(rest) else {
            return refuse(format!(
                "{path}:{number}: `{line}` is a `name` this does not read — it wants \
                 `name = \"<Ident>\"`, one plain double-quoted string with no escapes. Pass --lib \
                 <Name>",
                path = path.display(),
            ));
        };
        if slot.is_some() {
            return refuse(format!(
                "{path}:{number}: a second `name` in one [[lean_lib]] block. Pass --lib <Name>",
                path = path.display(),
            ));
        }
        *slot = Some(name);
    }
    close(&mut open, &mut names, path, text.lines().count())?;

    if names.is_empty() {
        return refuse(format!(
            "{}: no [[lean_lib]] block. A package with no library has no modules to document; if \
             it has one under another spelling, pass --lib <Name>",
            path.display(),
        ));
    }
    Ok(names)
}

/// Ends the current `[[lean_lib]]` block, if there is one.
///
/// A block with no `name` is refused rather than skipped: Lake defaults the
/// library's name to the package's, and guessing that here would produce a
/// module root nobody wrote down.
fn close(
    open: &mut Option<Option<String>>,
    names: &mut Vec<String>,
    path: &Path,
    number: usize,
) -> Result<(), Failure> {
    match open.take() {
        None => Ok(()),
        Some(Some(name)) => {
            names.push(name);
            Ok(())
        }
        Some(None) => refuse(format!(
            "{path}: the [[lean_lib]] block ending at line {number} has no `name` key. Lake fills \
             that in from the package, and inventing the value here would glob a module root \
             nobody wrote down. Pass --lib <Name>",
            path = path.display(),
        )),
    }
}

/// The text after `name` and its `=`, when the line's key is exactly `name`.
///
/// `name_of = "x"` is a different key and must not match, which is why the
/// prefix is checked against a boundary rather than with `starts_with("name")`.
fn key_is_name(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("name")?;
    let rest = rest.trim_start();
    rest.strip_prefix('=').map(str::trim_start)
}

/// `"<value>"`, optionally followed by a `#` comment, and nothing else.
fn plain_string(text: &str) -> Option<String> {
    let body = text.strip_prefix('"')?;
    let (value, rest) = body.split_once('"')?;
    if value.is_empty() || value.contains('\\') {
        return None;
    }
    let rest = rest.trim();
    if rest.is_empty() || rest.starts_with('#') {
        Some(value.to_owned())
    } else {
        None
    }
}

/// Exit 3, as [`read_libraries`]'s heading says.
fn refuse<T>(message: String) -> Result<T, Failure> {
    Err(Failure::Refused { code: 3, message })
}
