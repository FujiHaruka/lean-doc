//! `litedoc4 watch`'s file server: **the bytes that were built, and nothing
//! else**.
//!
//! Feature sweep A-2. It is a few hundred lines over [`std::net::TcpListener`]
//! and it stays that way on purpose — this tree's whole dependency list is
//! `serde`, `serde_json` and `sha2`, and a static file server is not a reason to
//! lengthen it (`Cargo.toml`, and A-1's `curl` decision one feature over).
//!
//! # No live-reload script is injected 【判断】
//!
//! The obvious feature is a snippet appended to every page that polls and
//! reloads the tab. It is not here, and the reason is the same one that keeps
//! `build` and `site` sharing one function: **what a developer is looking at has
//! to be what they will publish.** A server that rewrites the HTML on its way
//! out makes the previewed page and the shipped page two different files, and
//! the difference is invisible exactly when it matters — a selector that only
//! matches because of the injected node, a script error the injected script
//! swallowed. `watch` prints a line per rebuild instead; the reload is a
//! keystroke.
//!
//! What the server *does* owe the reader is that the keystroke works:
//! `Cache-Control: no-store` on every response, because doc-gen4 #389 is
//! literally "local build does not refresh after code change" and a 200 the
//! browser served out of its own cache is that bug with the fix already applied.
//!
//! # The port is refused, never moved 【判断】
//!
//! A taken port is an error with a name ([`bind`]), not a reason to try the next
//! one. An address that moves between runs is worse than one that is refused,
//! because the tab the reader already has open then points at nothing and
//! nothing says so. [`DEFAULT_PORT`] carries the rest of the reasoning.
//!
//! # Loopback only
//!
//! `127.0.0.1`, not `0.0.0.0`: this serves a half-finished documentation site
//! out of somebody's working tree, and binding every interface publishes it to
//! whatever network the machine is on without anybody having asked.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::thread;

use crate::Failure;

/// The port `watch` binds unless `--port` says otherwise.
///
/// **Not 8899**: `tools/check-site-browser.ts` drives puppeteer there and
/// CLAUDE.md records that it sometimes leaks the listener, so a default that
/// collided with it would fail for a reason that has nothing to do with this
/// command. Not 3000 / 8000 / 8080 / 5173 / 4173 either — those are where every
/// other development server in a working tree already is, and the first thing a
/// default port must do is be free.
pub(crate) const DEFAULT_PORT: u16 = 8484;

/// How much of a request head is read before it is answered with 400.
///
/// A request line plus headers; the body is never read because no method here
/// has one.
const MAX_HEAD_BYTES: u64 = 8 * 1024;

/// The listener, or a refusal that names the port.
pub(crate) fn bind(port: u16) -> Result<TcpListener, Failure> {
    TcpListener::bind(("127.0.0.1", port)).map_err(|source| Failure::Refused {
        code: 3,
        message: format!(
            "127.0.0.1:{port}: {source}. The port is refused rather than moved to the next free \
             one — an address that changes between runs leaves the browser tab you already have \
             open pointing at nothing. Pass --port <n>, or find what is holding it with `lsof -nP \
             -iTCP:{port} -sTCP:LISTEN`",
        ),
    })
}

/// Answers requests until the process ends. Runs on its own thread.
pub(crate) fn serve(listener: &TcpListener, root: &Path) {
    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                let root = root.to_owned();
                // One thread per connection. A browser opens several at once for
                // one page (the HTML, the CSS, the script), and a single-threaded
                // accept loop makes each of them wait for the one before it —
                // which on a 34 MB site reads as the server being slow.
                let spawned = thread::Builder::new()
                    .name("litedoc4-http".to_owned())
                    .spawn(move || {
                        if let Err(source) = answer(&stream, &root) {
                            // A client that went away mid-response is ordinary
                            // (a reload cancels the one in flight), so this is a
                            // note and never a failure of the run.
                            eprintln!("watch   http: {source}");
                        }
                    });
                if let Err(source) = spawned {
                    eprintln!("watch   http: cannot start a connection thread: {source}");
                }
            }
            Err(source) => eprintln!("watch   http: cannot accept a connection: {source}"),
        }
    }
}

// ------------------------------------------------------------------ answering

/// One request, from the first byte to the last.
fn answer(stream: &TcpStream, root: &Path) -> std::io::Result<()> {
    let Some(line) = head(stream)? else {
        let empty = Response::text(400, "Bad Request", "no request line");
        return write_response(stream, &empty, true);
    };
    let mut parts = line.split(' ');
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    let body = match method {
        "GET" => true,
        "HEAD" => false,
        _ => {
            return write_response(
                stream,
                &Response::text(
                    405,
                    "Method Not Allowed",
                    "this server reads a directory of files; GET and HEAD are all it can do",
                ),
                true,
            );
        }
    };
    write_response(stream, &respond(root, target), body)
}

/// The request line, with the rest of the head read and discarded.
///
/// Bounded: a connection that never sends a blank line is answered rather than
/// waited on, which is the same rule the loop upstairs follows — nothing here
/// may look like a hang.
fn head(stream: &TcpStream) -> std::io::Result<Option<String>> {
    let mut reader = BufReader::new(stream).take(MAX_HEAD_BYTES);
    let mut first = String::new();
    let mut line = String::new();
    loop {
        line.clear();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            break;
        }
        let text = line.trim_end_matches(['\r', '\n']);
        if first.is_empty() {
            if text.is_empty() {
                continue;
            }
            first = text.to_owned();
            continue;
        }
        if text.is_empty() {
            break;
        }
    }
    Ok((!first.is_empty()).then_some(first))
}

/// What the server says about one request target.
fn respond(root: &Path, target: &str) -> Response {
    if !root.is_dir() {
        // The first build has not finished, or it failed. **Said, not hung on**:
        // doc-gen4 #404 was a ten-minute silence its reporter read as a hang, and
        // an empty page with no explanation is the browser's version of it.
        return Response::text(
            503,
            "Service Unavailable",
            "the site has not been generated yet — the first build is still running, or it \
             stopped. The window running `litedoc4 watch` says which",
        );
    }
    match route(root, target) {
        Route::File(path) => match fs::read(&path) {
            Ok(bytes) => Response {
                status: 200,
                reason: "OK",
                kind: content_type(&path),
                body: bytes,
            },
            Err(source) => Response::text(
                500,
                "Internal Server Error",
                &format!("{}: {source}", path.display()),
            ),
        },
        Route::Missing => not_found(root, target),
        Route::Bad(why) => Response::text(400, "Bad Request", why),
        Route::Outside(why) => Response::text(403, "Forbidden", why),
    }
}

/// The site's own `404.html` when it has one, so that a wrong URL looks the way
/// it will look once the site is published.
fn not_found(root: &Path, target: &str) -> Response {
    let page = root.join("404.html");
    match fs::read(&page) {
        Ok(bytes) => Response {
            status: 404,
            reason: "Not Found",
            kind: "text/html; charset=utf-8",
            body: bytes,
        },
        Err(_) => Response::text(404, "Not Found", &format!("no such file: {target}")),
    }
}

/// What one request target resolves to, before the file is read.
#[derive(Debug, PartialEq, Eq)]
enum Route {
    File(PathBuf),
    /// Nothing there: 404.
    Missing,
    /// Malformed: 400.
    Bad(&'static str),
    /// Well formed and pointing out of the tree: 403.
    Outside(&'static str),
}

/// The file a request target names, **or a refusal**.
///
/// Two layers, and both are tested, because they fail differently. The first is
/// [`segments`], which never lets a `..` become part of a path at all. The
/// second is the `starts_with` below, which catches the case the first cannot
/// see: a symlink *inside* the site pointing out of it, where every component is
/// innocent and the resolved file is somebody's private key. A generated site
/// holds no symlinks, which is the argument for leaving the check out and
/// exactly why it is in — the site is a directory a person can put anything in.
fn route(root: &Path, target: &str) -> Route {
    let segments = match segments(target) {
        Ok(segments) => segments,
        Err(route) => return route,
    };
    let mut path = root.to_owned();
    for segment in &segments {
        path.push(segment);
    }
    if path.is_dir() {
        path.push("index.html");
    }
    if let (Ok(root), Ok(resolved)) = (fs::canonicalize(root), fs::canonicalize(&path))
        && !resolved.starts_with(&root)
    {
        return Route::Outside(
            "that path resolves outside the site directory, so it is not this server's to serve",
        );
    }
    if path.is_file() {
        Route::File(path)
    } else {
        Route::Missing
    }
}

/// The path components of a request target, percent-decoded and checked.
///
/// **Decoded first, checked second**, which is the order that matters: `%2e%2e`
/// is `..` and `%2f` is `/`, so a check that ran before the decoding would pass
/// both. Everything that survives is a single component with no separator in it,
/// so the join in [`route`] cannot leave the tree.
fn segments(target: &str) -> Result<Vec<String>, Route> {
    // The query and the fragment are not part of the file name. A fragment never
    // reaches a server, but a hand-written request can carry one.
    let path = target.split(['?', '#']).next().unwrap_or_default();
    if !path.starts_with('/') {
        return Err(Route::Bad(
            "a request target has to be an absolute path (this server answers `GET /x.html`, not \
             an absolute URL and not a proxy request)",
        ));
    }
    let mut out: Vec<String> = Vec::new();
    for raw in path.split('/') {
        let decoded =
            decode(raw).ok_or(Route::Bad("a % escape in the path is not two hex digits"))?;
        match decoded.as_str() {
            "" | "." => {}
            ".." => {
                return Err(Route::Outside(
                    "`..` is not resolved here: this server answers out of one directory and a \
                     path that climbs above it has no answer",
                ));
            }
            _ => {
                if decoded.contains(['/', '\\']) || decoded.contains('\0') {
                    return Err(Route::Outside(
                        "an escaped path separator is not a file name: `%2f`, `%5c` and a NUL are \
                         refused rather than joined",
                    ));
                }
                out.push(decoded);
            }
        }
    }
    Ok(out)
}

/// One path segment with its `%XX` escapes resolved, or `None` when an escape is
/// malformed.
///
/// Bytes, not characters: a percent escape names a byte, and a UTF-8 name
/// arrives as several of them. The result is lossy-decoded because a file name
/// that is not UTF-8 cannot be compared with one that is on the way to a
/// refusal anyway.
fn decode(segment: &str) -> Option<String> {
    if !segment.contains('%') {
        return Some(segment.to_owned());
    }
    let raw = segment.as_bytes();
    let mut bytes: Vec<u8> = Vec::with_capacity(raw.len());
    let mut at = 0;
    while at < raw.len() {
        if raw[at] == b'%' {
            let hex = raw.get(at + 1..at + 3)?;
            let text = std::str::from_utf8(hex).ok()?;
            bytes.push(u8::from_str_radix(text, 16).ok()?);
            at += 3;
        } else {
            bytes.push(raw[at]);
            at += 1;
        }
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// The `Content-Type` for a file of the site.
///
/// The site is HTML, CSS, one script, JSON and one SVG (`litedoc4_render::ASSETS`
/// and `litedoc4_global`'s artifacts); the rest of the list is what a person
/// drops into a site directory. Anything unrecognised is served as bytes rather
/// than guessed at.
fn content_type(path: &Path) -> &'static str {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    // Lowercased because APFS is case-insensitive by default, so the extension
    // the file has and the extension a comparison sees are two questions
    // (CLAUDE.md's `case_sensitive_file_extension_comparisons`).
    let name = name.to_lowercase();
    let extension = name.rsplit_once('.').map_or("", |(_, tail)| tail);
    match extension {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "txt" | "md" => "text/plain; charset=utf-8",
        "xml" => "application/xml",
        _ => "application/octet-stream",
    }
}

// ------------------------------------------------------------------ responses

/// One answer, ready to be written.
struct Response {
    status: u16,
    reason: &'static str,
    kind: &'static str,
    body: Vec<u8>,
}

impl Response {
    fn text(status: u16, reason: &'static str, body: &str) -> Self {
        Self {
            status,
            reason,
            kind: "text/plain; charset=utf-8",
            body: format!("{body}\n").into_bytes(),
        }
    }
}

/// The head and, for `GET`, the body.
///
/// `Content-Length` is the file's length on a `HEAD` too — that is what the
/// header means — and `Connection: close` because there is no keep-alive here:
/// one request per connection is what makes the read above bounded.
fn write_response(stream: &TcpStream, response: &Response, body: bool) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: \
         no-store\r\nConnection: close\r\n\r\n",
        response.status,
        response.reason,
        response.kind,
        response.body.len(),
    );
    let mut out = stream;
    out.write_all(head.as_bytes())?;
    if body {
        out.write_all(&response.body)?;
    }
    out.flush()
}

// --------------------------------------------------------------------- tests

/// The routing, the refusals and one real socket.
///
/// All of it owns its input — a directory this module writes and a listener on
/// port 0 — so it is a test rather than a gate. What the gate has instead is the
/// measurement target and a running `watch` (`tools/watch-gate.sh`).
#[cfg(test)]
mod tests {
    use std::net::TcpStream;

    use super::*;

    /// A three-file site, and the temporary directory it lives in.
    struct Site {
        root: PathBuf,
    }

    impl Site {
        fn new(what: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("litedoc4-httpd-{}-{what}", std::process::id(),));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(root.join("Pkg")).expect("the directory is creatable");
            fs::write(root.join("index.html"), "<h1>index</h1>").expect("writable");
            fs::write(root.join("404.html"), "<h1>not found</h1>").expect("writable");
            fs::write(root.join("style.css"), "body{}").expect("writable");
            fs::write(root.join("Pkg/Basic.html"), "<h1>Pkg.Basic</h1>").expect("writable");
            fs::write(root.join("Pkg/index.html"), "<h1>Pkg</h1>").expect("writable");
            Self { root }
        }
    }

    impl Drop for Site {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn a_directory_is_its_index_and_a_file_is_itself() {
        let site = Site::new("routing");
        assert_eq!(
            route(&site.root, "/"),
            Route::File(site.root.join("index.html")),
        );
        assert_eq!(
            route(&site.root, "/Pkg/"),
            Route::File(site.root.join("Pkg").join("index.html")),
        );
        assert_eq!(
            route(&site.root, "/Pkg"),
            Route::File(site.root.join("Pkg").join("index.html")),
            "a directory without the trailing slash is the same directory",
        );
        assert_eq!(
            route(&site.root, "/Pkg/Basic.html"),
            Route::File(site.root.join("Pkg").join("Basic.html")),
        );
        // The query is not part of the file name: the site's own script appends
        // one to break a cache.
        assert_eq!(
            route(&site.root, "/style.css?v=2"),
            Route::File(site.root.join("style.css")),
        );
    }

    #[test]
    fn a_file_that_is_not_there_is_missing_and_not_an_error() {
        let site = Site::new("missing");
        assert_eq!(route(&site.root, "/Pkg/Other.html"), Route::Missing);
        assert_eq!(route(&site.root, "/nope/"), Route::Missing);
    }

    /// The correctness test the design owes: **a request may not name a file
    /// outside the directory being served**, in every spelling that reaches this
    /// function.
    #[test]
    fn a_path_that_leaves_the_site_is_refused_in_every_spelling() {
        let site = Site::new("traversal");
        let outside = site.root.parent().expect("a parent").join("secret.txt");
        fs::write(&outside, "not yours").expect("writable");

        for target in [
            "/../secret.txt",
            "/Pkg/../../secret.txt",
            "/%2e%2e/secret.txt",
            "/%2E%2E/secret.txt",
            "/..%2fsecret.txt",
            "/%2f%2e%2e%2fsecret.txt",
            "/Pkg%5c..%5csecret.txt",
        ] {
            match route(&site.root, target) {
                Route::Outside(_) => {}
                other => panic!("{target} was not refused: {other:?}"),
            }
        }
        // And the two failures that are *not* traversal keep their own answer,
        // so a 400 and a 403 do not become one word.
        assert!(matches!(route(&site.root, "index.html"), Route::Bad(_)));
        assert!(matches!(route(&site.root, "/%zz.html"), Route::Bad(_)));
        let _ = fs::remove_file(&outside);
    }

    #[test]
    fn a_symlink_out_of_the_site_is_refused_even_though_every_component_is_innocent() {
        let site = Site::new("symlink");
        let outside = site
            .root
            .parent()
            .expect("a parent")
            .join("symlink-secret.txt");
        fs::write(&outside, "not yours").expect("writable");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, site.root.join("escape.txt")).expect("symlink");
        #[cfg(unix)]
        match route(&site.root, "/escape.txt") {
            Route::Outside(_) => {}
            other => panic!("the symlink was served: {other:?}"),
        }
        let _ = fs::remove_file(&outside);
    }

    #[test]
    fn the_content_type_follows_the_extension_and_falls_back_to_bytes() {
        assert_eq!(
            content_type(Path::new("a/b.html")),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            content_type(Path::new("a/b.HTML")),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            content_type(Path::new("style.css")),
            "text/css; charset=utf-8"
        );
        assert_eq!(
            content_type(Path::new("declarations/name-map.json")),
            "application/json; charset=utf-8",
        );
        assert_eq!(content_type(Path::new("favicon.svg")), "image/svg+xml");
        assert_eq!(
            content_type(Path::new("LICENSE")),
            "application/octet-stream"
        );
    }

    /// One real connection, because the routing above says nothing about the
    /// bytes on the wire — the status line, the length and the header that keeps
    /// a reload from being served out of the browser's cache.
    #[test]
    fn a_real_request_gets_the_file_and_a_real_miss_gets_the_sites_own_404() {
        let site = Site::new("socket");
        // Port 0: the kernel picks a free one, so this test cannot collide with
        // anything on the machine running it — including another copy of itself.
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("a free port");
        let port = listener.local_addr().expect("bound").port();
        let root = site.root.clone();
        let served = thread::spawn(move || serve(&listener, &root));

        let ok = request(port, "GET /Pkg/Basic.html HTTP/1.1");
        assert!(ok.starts_with("HTTP/1.1 200 OK\r\n"), "{ok}");
        assert!(
            ok.contains("Content-Type: text/html; charset=utf-8\r\n"),
            "{ok}"
        );
        assert!(ok.contains("Content-Length: 18\r\n"), "{ok}");
        assert!(ok.contains("Cache-Control: no-store\r\n"), "{ok}");
        assert!(ok.ends_with("<h1>Pkg.Basic</h1>"), "{ok}");

        let missing = request(port, "GET /Pkg/Nope.html HTTP/1.1");
        assert!(
            missing.starts_with("HTTP/1.1 404 Not Found\r\n"),
            "{missing}"
        );
        assert!(
            missing.ends_with("<h1>not found</h1>"),
            "the site's own 404 page, so a wrong URL looks the way it will once published: \
             {missing}",
        );

        let refused = request(port, "GET /../secret.txt HTTP/1.1");
        assert!(
            refused.starts_with("HTTP/1.1 403 Forbidden\r\n"),
            "{refused}"
        );

        let head = request(port, "HEAD /Pkg/Basic.html HTTP/1.1");
        assert!(head.starts_with("HTTP/1.1 200 OK\r\n"), "{head}");
        assert!(head.contains("Content-Length: 18\r\n"), "{head}");
        assert!(head.ends_with("\r\n\r\n"), "a HEAD carries no body: {head}");

        let other = request(port, "PUT /index.html HTTP/1.1");
        assert!(
            other.starts_with("HTTP/1.1 405 Method Not Allowed\r\n"),
            "{other}",
        );
        drop(served);
    }

    #[test]
    fn a_site_that_is_not_there_yet_says_so_rather_than_answering_nothing() {
        let root =
            std::env::temp_dir().join(format!("litedoc4-httpd-{}-absent", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let answer = respond(&root, "/");
        assert_eq!(answer.status, 503);
        assert!(
            String::from_utf8_lossy(&answer.body).contains("has not been generated yet"),
            "an empty page with no explanation is the browser's version of a hang",
        );
    }

    /// One request, one connection, the whole answer as text.
    fn request(port: u16, line: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("the server is listening");
        stream
            .write_all(format!("{line}\r\nHost: localhost\r\n\r\n").as_bytes())
            .expect("writable");
        stream.flush().expect("flushable");
        let mut answer = Vec::new();
        stream.read_to_end(&mut answer).expect("readable");
        String::from_utf8_lossy(&answer).into_owned()
    }
}
