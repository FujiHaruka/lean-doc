//! The `lean-doc` command line tool.
//!
//! **The subcommand surface is milestone M4's.** What is here is the one
//! subcommand M1 needs to be judged: `render`, which turns an IR tree into a
//! tree of module pages and is what `tools/render-compare.sh` runs against the
//! frozen prototype's output. Everything else — extraction, the global
//! artifacts, the incremental pipeline, the resident server — arrives with its
//! own milestone, and guessing at its flags now would only have to be undone.
//!
//! Two flags are deliberately more awkward than the prototype's:
//!
//! - **`--only` and `--only-from` are the same option in two spellings, and
//!   `--only-from` an empty file renders nothing.** `render.ts` could not say
//!   "no modules": zero `--only` flags meant every module, so the incremental
//!   pipeline had to guard the call in shell (plan §5). A regeneration set is
//!   usually empty, so that hole is on the common path.
//! - **One of `--link-index` and `--no-link-index` is required.** The
//!   dependency map is not optional in the product (plan 決定 4); leaving it
//!   out costs 150 of 432 pages their correct bytes, and it does so silently.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lean_doc_render::{ModuleSet, RenderOptions, render_site};

const USAGE: &str = "\
usage: lean-doc render --ir <dir> --pages <dir> --source-url <url>
                       (--link-index <file> | --no-link-index)
                       [--only <Module>]... [--only-from <file>]

  --ir           an IR tree written by the extractor (schema 4)
  --pages        where the pages go; directories are created
  --source-url   https://host/owner/repo/blob/<40-hex-rev>
  --link-index   the dependency closure's name -> module map (.lidx)
  --only         render only this module; repeatable
  --only-from    render only the modules named in this file, one per line.
                 An empty file renders nothing.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Usage(message)) => {
            eprintln!("lean-doc: {message}\n\n{USAGE}");
            ExitCode::from(2)
        }
        Err(Failure::Failed(message)) => {
            eprintln!("lean-doc: {message}");
            ExitCode::FAILURE
        }
    }
}

enum Failure {
    /// The command line is wrong. Exit 2, as the prototype does.
    Usage(String),
    Failed(String),
}

fn usage<T>(message: impl Into<String>) -> Result<T, Failure> {
    Err(Failure::Usage(message.into()))
}

fn run(args: &[String]) -> Result<(), Failure> {
    match args.first().map(String::as_str) {
        Some("render") => render(&args[1..]),
        Some("--help" | "-h") | None => {
            println!("{USAGE}");
            Ok(())
        }
        Some("--version") => {
            println!("lean-doc {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some(other) => usage(format!("unknown subcommand `{other}`")),
    }
}

fn render(args: &[String]) -> Result<(), Failure> {
    let mut ir: Option<PathBuf> = None;
    let mut pages: Option<PathBuf> = None;
    let mut source_url: Option<String> = None;
    let mut link_index: Option<PathBuf> = None;
    let mut no_link_index = false;
    // `None` until an `--only` of either spelling appears: the distinction
    // between "no subset asked for" and "a subset that came out empty" is the
    // whole point (plan §5).
    let mut only: Option<BTreeSet<String>> = None;

    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let mut value = |flag: &str| -> Result<String, Failure> {
            match rest.next() {
                Some(value) => Ok(value.clone()),
                None => usage(format!("{flag} needs a value")),
            }
        };
        match arg.as_str() {
            "--ir" => ir = Some(value("--ir")?.into()),
            "--pages" => pages = Some(value("--pages")?.into()),
            "--source-url" => source_url = Some(value("--source-url")?),
            "--link-index" => link_index = Some(value("--link-index")?.into()),
            "--no-link-index" => no_link_index = true,
            "--only" => {
                only.get_or_insert_with(BTreeSet::new)
                    .insert(value("--only")?);
            }
            "--only-from" => {
                let path = PathBuf::from(value("--only-from")?);
                let text = std::fs::read_to_string(&path)
                    .map_err(|e| Failure::Failed(format!("{}: {e}", path.display())))?;
                let ModuleSet::These(names) = ModuleSet::from_lines(&text) else {
                    unreachable!("from_lines always names a set")
                };
                only.get_or_insert_with(BTreeSet::new).extend(names);
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => return usage(format!("unknown argument `{other}`")),
        }
    }

    let Some(ir) = ir else {
        return usage("--ir is required");
    };
    let Some(pages) = pages else {
        return usage("--pages is required");
    };
    // The prototype refuses too: the source URL is configuration that no IR
    // carries, and a page written without it links every declaration to `/`.
    let Some(source_url) = source_url.filter(|url| !url.is_empty()) else {
        return usage(
            "--source-url is required: doc-gen4 reads it from lake plus git, and it is not in the IR",
        );
    };
    if link_index.is_some() == no_link_index {
        return usage(
            "pass --link-index <file>, or --no-link-index to say so on purpose: without the \
             dependency map 150 of the target package's 432 pages change bytes (plan 決定 4)",
        );
    }

    let only = match only {
        Some(names) => ModuleSet::These(names),
        None => ModuleSet::All,
    };
    let summary = render_site(&RenderOptions {
        ir: &ir,
        pages: &pages,
        source_url: &source_url,
        link_index: link_index.as_deref().map(Path::new),
        only: &only,
    })
    .map_err(|e| Failure::Failed(e.to_string()))?;

    // Counts, not a checkmark: every number here is a denominator something
    // else gets quoted against.
    println!(
        "modules {}/{}  declarations {}/{} ({} suppressed)  module docs {}  bytes {}",
        summary.pages_written,
        summary.modules_in_ir,
        summary.declarations_rendered,
        summary.declarations_in_ir,
        summary.declarations_suppressed,
        summary.module_docs_rendered,
        summary.bytes_written,
    );
    println!(
        "known {}  link index {}  known modules {}",
        summary.known_entries, summary.link_index_entries, summary.known_modules,
    );
    Ok(())
}
