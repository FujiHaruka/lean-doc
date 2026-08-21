//! `litedoc4.toml` — what a package says about its own site.
//!
//! `docs/plans/feature-sweep.md` C-3【決定 3】, doc-gen4 #43 and #102.
//!
//! # Why a file and not a flag
//!
//! [`crate::SiteMeta::of_modules`] derives the title from the module names and
//! says why there is no `--title`: three commands render (`build`, `site`,
//! `render`) and a flag forgotten on one of them makes two of them disagree
//! about what the site is called. That argument is right, and it is an argument
//! against *flags*, not against configuration — a file inside the package is
//! read by whichever command is pointed at the package, so there is nothing to
//! forget per command.
//!
//! What replaces "forgotten flag" is "unread file", and that is what
//! `tools/config-gate.sh` is for: it renders one package through all three
//! commands and compares what they say.
//!
//! # The whole surface
//!
//! ```toml
//! title = "MyPkg"          # the top bar, and the second half of every <title>
//! index = "docs/index.md"  # Markdown to put at the top of the site's index
//! ```
//!
//! Both optional. An absent file, an empty file and a file with neither key are
//! the same thing: [`SiteConfig::default`], which is what every site rendered
//! before C-3 already had.
//!
//! # What is deliberately not here
//!
//! `--source-url` and the dependency documentation map. Both are configuration
//! too, and both belong to the *checkout* rather than to the package: the
//! source URL carries a git revision that changes on every commit, and the
//! deps map is derived from `lake-manifest.json`. A value that a package
//! commits has to be one that stays true across commits.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The file's name under the package root.
pub const CONFIG_FILE: &str = "litedoc4.toml";

/// What one package configured, with the `index` file already read.
///
/// **Reading the Markdown here rather than at the point of use** is the whole
/// reason this is a type and not a struct of two `Option<String>`s: `index` is
/// a path relative to the package root, and resolving it anywhere else would be
/// a second place that knows what it is relative to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SiteConfig {
    /// `title`, when the file set one.
    pub title: Option<String>,
    /// The **contents** of the file `index` named, not its path.
    pub index_markdown: Option<String>,
    /// Where that Markdown came from, for diagnostics.
    pub index_path: Option<PathBuf>,
}

impl SiteConfig {
    /// The configuration of a package that configured nothing.
    ///
    /// The same value as [`SiteConfig::default`], as a `const` — so a caller
    /// that needs a `&'static SiteConfig` (an options struct's default, a test)
    /// has one without keeping a local alive next to it.
    pub const EMPTY: Self = Self {
        title: None,
        index_markdown: None,
        index_path: None,
    };

    /// Reads `<root>/litedoc4.toml`, or [`SiteConfig::default`] when `root` is
    /// `None` or holds no such file.
    ///
    /// **A file that is there and does not parse is an error**, and so is an
    /// `index` naming a file that is not there. The alternative — carrying on
    /// with the derived title — is a site that silently ignores what the
    /// package asked for, which is the failure this whole item exists to
    /// remove.
    ///
    /// # Errors
    ///
    /// [`Error::Parse`] for a malformed file, [`Error::Io`] for one that cannot
    /// be read and for an `index` that names nothing.
    pub fn read(root: Option<&Path>) -> Result<Self, Error> {
        let Some(root) = root else {
            return Ok(Self::default());
        };
        let path = root.join(CONFIG_FILE);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            // Absent is the ordinary case: most packages configure nothing.
            Err(source) if source.kind() == io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(source) => return Err(Error::Io { path, source }),
        };
        let file: File = basic_toml::from_str(&text).map_err(|source| Error::Parse {
            path: path.clone(),
            message: source.to_string(),
        })?;

        let (index_markdown, index_path) = match file.index {
            Some(relative) => {
                let resolved = root.join(&relative);
                let markdown = fs::read_to_string(&resolved).map_err(|source| Error::Io {
                    path: resolved.clone(),
                    source,
                })?;
                (Some(markdown), Some(resolved))
            }
            None => (None, None),
        };
        Ok(Self {
            // An empty title is not a title. `title = ""` would otherwise put a
            // blank where every page names the site.
            title: file.title.filter(|title| !title.trim().is_empty()),
            index_markdown,
            index_path,
        })
    }
}

/// The file's shape. Separate from [`SiteConfig`] because that one carries the
/// Markdown and this one carries the path to it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    title: Option<String>,
    index: Option<String>,
}

/// Why a configuration could not be read.
#[derive(Debug)]
pub enum Error {
    Io { path: PathBuf, source: io::Error },
    Parse { path: PathBuf, message: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Parse { path, message } => write!(f, "{}: {message}", path.display()),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Parse { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CONFIG_FILE, Error, SiteConfig};
    use std::fs;
    use std::path::{Path, PathBuf};

    struct Dir(PathBuf);

    impl Dir {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("litedoc4-config-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("a scratch directory");
            Self(path)
        }
        fn write(&self, name: &str, text: &str) -> &Self {
            let path = self.0.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("a parent");
            }
            fs::write(path, text).expect("a file");
            self
        }
        fn read(&self) -> Result<SiteConfig, Error> {
            SiteConfig::read(Some(&self.0))
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn no_root_and_no_file_are_the_same_answer() {
        assert_eq!(
            SiteConfig::read(None).expect("no root"),
            SiteConfig::default()
        );
        let dir = Dir::new("absent");
        assert_eq!(dir.read().expect("no file"), SiteConfig::default());
    }

    #[test]
    fn both_keys_are_read_and_the_index_arrives_as_text() {
        let dir = Dir::new("both");
        dir.write(
            CONFIG_FILE,
            "title = \"MyPkg\"\nindex = \"docs/index.md\"\n",
        )
        .write("docs/index.md", "# Hello\n");
        let config = dir.read().expect("a valid file");
        assert_eq!(config.title.as_deref(), Some("MyPkg"));
        assert_eq!(config.index_markdown.as_deref(), Some("# Hello\n"));
        assert_eq!(
            config.index_path.as_deref(),
            Some(dir.0.join("docs/index.md").as_path())
        );
    }

    /// An empty title is not a title: it would put a blank where every page
    /// names the site, which is worse than the derived name.
    #[test]
    fn a_blank_title_falls_back_to_the_derived_one() {
        let dir = Dir::new("blank");
        dir.write(CONFIG_FILE, "title = \"   \"\n");
        assert_eq!(dir.read().expect("valid").title, None);
    }

    /// The three ways the file is wrong, all of which stop the build. Carrying
    /// on with the derived title is the silent failure C-3 exists to remove.
    #[test]
    fn a_file_that_is_wrong_is_an_error_and_not_a_default() {
        let bad = Dir::new("bad-toml");
        bad.write(CONFIG_FILE, "title = \n");
        assert!(matches!(bad.read(), Err(Error::Parse { .. })));

        let unknown = Dir::new("unknown-key");
        unknown.write(CONFIG_FILE, "titel = \"typo\"\n");
        assert!(
            matches!(unknown.read(), Err(Error::Parse { .. })),
            "a misspelled key has to be reported, not ignored"
        );

        let missing = Dir::new("missing-index");
        missing.write(CONFIG_FILE, "index = \"docs/nope.md\"\n");
        assert!(matches!(missing.read(), Err(Error::Io { .. })));
    }

    /// The error names the file, because "could not read the configuration" with
    /// no path is a bisection.
    #[test]
    fn the_error_names_the_path() {
        let dir = Dir::new("names");
        dir.write(CONFIG_FILE, "index = \"docs/nope.md\"\n");
        let error = dir.read().expect_err("no such index");
        let text = error.to_string();
        assert!(text.contains("nope.md"), "{text}");
        assert!(Path::new(&text.split(':').next().unwrap_or("")).is_absolute());
    }
}
