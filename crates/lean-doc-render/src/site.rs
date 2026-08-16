//! The run: an IR tree in, a tree of module pages out.
//!
//! Ported from `experiments/stage7d/render.ts` (frozen): the main body,
//! 1981-2136, minus the timing instrumentation and minus the flatten probe
//! (`render.ts:2120` exists to defeat V8's rope representation and has no
//! meaning here — the source even carries a literal NUL byte for it).
//!
//! # The order the maps are built in is behaviour
//!
//! `known` has three sources and they are not commutative (`render.ts:2001-2036`):
//! the dependency slices, then every declaration of every module — which
//! **overwrites** — then every reference the extractor resolved, which **fills
//! gaps only**. [`NameIndexBuilder`] enforces the difference at the call site;
//! this module's job is to feed it in that order.
//!
//! # Two things that are read for the whole site before any page is written
//!
//! Every module file is read even when only one page is being rendered.
//! `known`, `knownModules` and [`Suppressed`] are site-wide, and a page
//! rendered against a partial map differs in the links it draws rather than
//! failing — which is a byte difference that no error message announces.
//! That is also why [`ModuleSet`] filters *pages*, not *reads*.
//!
//! # `--only` with nothing in it means nothing
//!
//! The prototype spelled the module filter as the presence of a repeated flag
//! (`ONLY.length > 0 ? new Set(ONLY) : null`), so passing zero modules meant
//! "render all 432". The incremental pipeline's shell had to guard the call
//! (`incremental.sh:367`), and an empty regeneration set is the common case
//! once revisions are out of the bytes. [`ModuleSet`] has no `Default` and its
//! empty case is a value, not the absence of one (plan §5).

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use lean_doc_ir::IrTree;

use crate::autolink::NameIndex;
use crate::decl::UnplaceableName;
use crate::external::ExternalLinks;
use crate::frame::SiteMeta;
use crate::link_index::LinkIndex;
use crate::page::{Suppressed, page_html, page_path};

/// Which modules get a page written.
///
/// **`These(empty)` writes nothing.** The variant exists so that "the caller
/// computed a set and it came out empty" cannot be spelled the same way as
/// "the caller did not ask for a subset".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModuleSet {
    /// Every module in the IR.
    All,
    /// Exactly these, and nothing else.
    These(BTreeSet<String>),
}

impl ModuleSet {
    /// One module name per line, as the incremental pipeline's render set is
    /// written. Blank lines are dropped; **an empty file is an empty set**.
    #[must_use]
    pub fn from_lines(text: &str) -> Self {
        Self::These(
            text.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect(),
        )
    }

    #[must_use]
    pub fn contains(&self, module: &str) -> bool {
        match self {
            Self::All => true,
            Self::These(names) => names.contains(module),
        }
    }
}

/// What a run needs to know that the IR does not carry.
#[derive(Clone, Copy, Debug)]
pub struct RenderOptions<'a> {
    /// The IR tree: `index.json`, `modules/`, `deps/`.
    pub ir: &'a Path,
    /// Where the pages go. Directories are created as needed.
    pub pages: &'a Path,
    /// The `https://host/owner/repo/blob/<rev>` prefix every `source` link is
    /// built from. Configuration, not IR — doc-gen4 reads it from lake plus
    /// git. Trailing slashes are stripped here, as `render.ts:286` does.
    ///
    /// Plan 決定 1: the revision has to be 40 hex digits or the acceptance
    /// oracle scores the tree lower.
    pub source_url: &'a str,
    /// Where each **dependency's** source lives (M7-c).
    ///
    /// Next to `source_url` because it is the same kind of thing one level out:
    /// that one says where *this* package's sources are, this one says where
    /// everything it imports keeps theirs. Both are configuration the IR does
    /// not carry — `lean-doc`'s `packages` module resolves this one from the
    /// target's `lake-manifest.json` and its toolchain.
    ///
    /// [`ExternalLinks::default`] is the pre-M7 renderer, byte for byte: every
    /// link into a dependency stays a relative page link to a page this site
    /// never writes. It is spelled as an empty map rather than an `Option`
    /// because the two mean the same thing here and one of them cannot be
    /// misread as "the default".
    pub external_links: &'a ExternalLinks,
    /// The dependency closure's `name -> module` map.
    ///
    /// `None` is a decision, not a default: without it 150 of the target
    /// package's 432 pages change bytes 【実測, plan 決定 4】. The product
    /// always passes one.
    pub link_index: Option<&'a Path>,
    /// Which modules get a page.
    pub only: &'a ModuleSet,
}

/// What a run did, in the units its inputs are counted in.
///
/// Every field is a denominator something else can be quoted against; a run
/// that reports "done" and nothing else cannot be compared to the prototype's
/// numbers at all.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenderSummary {
    pub modules_in_ir: usize,
    pub declarations_in_ir: usize,
    /// Names that are some declaration's member, over the whole site.
    pub declarations_suppressed: usize,
    pub pages_written: usize,
    /// Declaration blocks on the pages that were written.
    pub declarations_rendered: usize,
    /// Module docstring blocks on the pages that were written.
    pub module_docs_rendered: usize,
    pub known_entries: usize,
    pub link_index_entries: usize,
    pub known_modules: usize,
    pub bytes_written: u64,
}

/// Reads the IR, builds the maps, and writes one page per wanted module.
pub fn render_site(options: &RenderOptions<'_>) -> Result<RenderSummary, Error> {
    let source_url = options.source_url.trim_end_matches('/');
    let tree = IrTree::open(options.ir)?;

    // `known`: dependency slices first, then the modules — declarations
    // overwrite, references only fill gaps.
    let mut builder = NameIndex::builder();
    for dep in tree.load_dep_maps()? {
        builder.dep_map(&dep);
    }
    let modules = tree.load_modules()?;
    for module in &modules {
        builder.module(module);
    }
    let links = match options.link_index {
        Some(path) => LinkIndex::read(path).map_err(|source| Error::Io {
            path: path.to_owned(),
            source,
        })?,
        None => LinkIndex::default(),
    };
    let index = builder.build(links, options.external_links.clone());

    // Site-wide, not per module: see [`Suppressed`].
    let suppressed = Suppressed::of_site(&modules);
    // Over **every** module of the IR, not the subset being rendered: an
    // incremental round that re-renders one page must not retitle the site.
    let site = SiteMeta::of_modules(modules.iter().map(|module| module.module.as_str()));

    let mut summary = RenderSummary {
        modules_in_ir: modules.len(),
        declarations_in_ir: modules.iter().map(|m| m.declarations.len()).sum(),
        declarations_suppressed: suppressed.len(),
        known_entries: index.len(),
        link_index_entries: index.link_index_len(),
        known_modules: index.known_module_count(),
        ..RenderSummary::default()
    };

    for module in &modules {
        if !options.only.contains(&module.module) {
            continue;
        }
        let html = page_html(module, &index, source_url, &suppressed, &site).map_err(|source| {
            Error::Unplaceable {
                module: module.module.clone(),
                source,
            }
        })?;
        let path = options.pages.join(page_path(&module.module));
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|source| Error::Io {
                path: dir.to_owned(),
                source,
            })?;
        }
        fs::write(&path, &html).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        summary.pages_written += 1;
        summary.bytes_written += html.len() as u64;
        summary.module_docs_rendered += module.module_docs.len();
        summary.declarations_rendered += module
            .declarations
            .iter()
            .filter(|d| !suppressed.contains(&d.name))
            .count();
    }
    Ok(summary)
}

/// Why a run stopped.
#[derive(Debug)]
pub enum Error {
    Ir(lean_doc_ir::Error),
    Io {
        path: PathBuf,
        source: io::Error,
    },
    /// A name that has to be linked and is in no module. doc-gen4 panics here;
    /// the page is not written instead, because a plausible `href` would be a
    /// wrong byte that costs a bisection to find.
    Unplaceable {
        module: String,
        source: UnplaceableName,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ir(source) => write!(f, "{source}"),
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Unplaceable { module, source } => write!(f, "rendering {module}: {source}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Ir(source) => Some(source),
            Self::Io { source, .. } => Some(source),
            Self::Unplaceable { source, .. } => Some(source),
        }
    }
}

impl From<lean_doc_ir::Error> for Error {
    fn from(source: lean_doc_ir::Error) -> Self {
        Self::Ir(source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_module_set_is_not_the_absence_of_one() {
        let empty = ModuleSet::These(BTreeSet::new());
        assert!(!empty.contains("Pkg.One"));
        assert!(ModuleSet::All.contains("Pkg.One"));
        assert_eq!(ModuleSet::from_lines(""), empty);
        assert_eq!(ModuleSet::from_lines("\n  \n"), empty);
        assert_eq!(
            ModuleSet::from_lines("Pkg.One\nPkg.Two\n"),
            ModuleSet::These(["Pkg.One".to_owned(), "Pkg.Two".to_owned()].into())
        );
        assert!(ModuleSet::from_lines("Pkg.One\n").contains("Pkg.One"));
        assert!(!ModuleSet::from_lines("Pkg.One\n").contains("Pkg.Two"));
    }
}
