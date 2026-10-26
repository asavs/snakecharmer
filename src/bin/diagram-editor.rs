//! diagram-editor — a dev-only local server for visually editing device
//! diagrams. Selects a supported model, serves its generated SVG with
//! draggable points in the browser, and on Save writes the new coordinates
//! back into the device file under `crates/razer-proto/src/devices/` and
//! regenerates the SVG assets for review.
//!
//! Run from the repo root:  cargo run --bin diagram-editor -- deathadder-v3
//!
//! Write-back never guesses: each edited shape is located in the razer-proto
//! sources (one file per device under `devices/`, plus lib.rs) by kind plus
//! its exact original number sequence. Same-count edits replace only the
//! numbers — formatting and comments stay untouched. Point deletions change
//! the count, so those rewrite the whole shape literal (its comments are
//! dropped). If the source drifted (stale browser tab), the save is rejected
//! wholesale before anything is written.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

const HTML: &str = include_str!("../../tools/diagram-editor.html");
const PROTO_SRC: &str = "crates/razer-proto/src";
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 1024 * 1024;

/// Every file a device shape may live in: the per-device modules first,
/// then lib.rs as a fallback.
fn source_files() -> Vec<std::path::PathBuf> {
    let mut v: Vec<std::path::PathBuf> = std::fs::read_dir(format!("{PROTO_SRC}/devices"))
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "rs"))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v.push(format!("{PROTO_SRC}/lib.rs").into());
    v
}

fn main() {
    let selection = std::env::args().nth(1);
    let spec = match select_model(selection.as_deref()) {
        Some(spec) => spec,
        None => {
            eprintln!("Choose a model with a diagram:");
            for spec in razer_proto::SUPPORTED.iter().filter(|s| s.diagram.is_some()) {
                eprintln!("  {}", razer_proto::diagram::asset_slug(spec.name));
            }
            std::process::exit(2);
        }
    };
    let name = spec.name;
    let slug = razer_proto::diagram::asset_slug(name);
    println!("editing {name}; saves update source and SVG assets for review");

    let listener = TcpListener::bind("127.0.0.1:7333")
        .or_else(|_| TcpListener::bind("127.0.0.1:0"))
        .expect("bind");
    let url = format!("http://{}", listener.local_addr().unwrap());
    println!("editor: {url}  (Ctrl+C to quit)");
    let _ = std::process::Command::new("cmd").args(["/C", "start", "", &url]).spawn();

    for stream in listener.incoming().flatten() {
        handle(stream, name, &slug);
    }
}

fn select_model(selection: Option<&str>) -> Option<razer_proto::DeviceSpec> {
    razer_proto::SUPPORTED.iter().copied().filter(|s| s.diagram.is_some()).find(|s| {
        selection.is_none_or(|selection| s.name.eq_ignore_ascii_case(selection)
            || razer_proto::diagram::asset_slug(s.name).eq_ignore_ascii_case(selection))
    })
}

fn handle(mut s: TcpStream, name: &str, slug: &str) {
    let timeout = Some(std::time::Duration::from_secs(5));
    if s.set_read_timeout(timeout).is_err() || s.set_write_timeout(timeout).is_err() {
        return;
    }
    let authority = match s.local_addr() {
        Ok(address) => address.to_string(),
        Err(_) => return,
    };
    let request = match read_request(&mut s, &authority) {
        Ok(request) => request,
        Err(message) => {
            respond(&mut s, "400 Bad Request", "text/plain", message.as_bytes());
            return;
        }
    };
    let path = request.path.split('?').next().unwrap_or("");
    let (status, ctype, payload): (&str, &str, Vec<u8>) = match (request.method.as_str(), path) {
        ("GET", "/") => ("200 OK", "text/html; charset=utf-8", HTML.as_bytes().to_vec()),
        ("GET", "/device") => ("200 OK", "text/plain; charset=utf-8", name.as_bytes().to_vec()),
        ("GET", "/notes") => {
            let text = std::fs::read_to_string("tools/diagram-notes.md").unwrap_or_default();
            ("200 OK", "text/plain; charset=utf-8", text.into_bytes())
        }
        ("GET", "/diagram.svg") => match std::fs::read(format!("docs/assets/{slug}.svg")) {
            Ok(svg) => ("200 OK", "image/svg+xml", svg),
            Err(e) => ("500 Internal Server Error", "text/plain", e.to_string().into_bytes()),
        },
        ("POST", "/save") => match save(&request.body) {
            Ok(log) => ("200 OK", "text/plain; charset=utf-8", log.into_bytes()),
            Err(log) => ("409 Conflict", "text/plain; charset=utf-8", log.into_bytes()),
        },
        ("POST", "/note") => match note(&request.body, name) {
            Ok(()) => ("200 OK", "text/plain; charset=utf-8", b"ok".to_vec()),
            Err(e) => ("500 Internal Server Error", "text/plain", e.into_bytes()),
        },
        _ => ("404 Not Found", "text/plain", b"not found".to_vec()),
    };
    respond(&mut s, status, ctype, &payload);
}

#[derive(Debug)]
struct Request {
    method: String,
    path: String,
    body: Vec<u8>,
}

/// Require the editor's own loopback Host and browser Origin before reading a
/// mutation body. JSON requests also prevent cross-origin form submissions.
fn read_request(input: &mut impl Read, authority: &str) -> Result<Request, &'static str> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    let (head, mut body) = loop {
        match input.read(&mut tmp) {
            Ok(0) | Err(_) => return Err("incomplete request headers"),
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
        }
        if let Some(pos) = find(&buf, b"\r\n\r\n") {
            if pos > MAX_HEADER_BYTES {
                return Err("request headers too large");
            }
            let head = String::from_utf8(buf[..pos].to_vec()).map_err(|_| "invalid request headers")?;
            break (head, buf[pos + 4..].to_vec());
        }
        if buf.len() > MAX_HEADER_BYTES {
            return Err("request headers too large");
        }
    };
    let mut lines = head.lines();
    let mut start = lines.next().ok_or("missing request line")?.split_whitespace();
    let method = start.next().ok_or("missing request method")?.to_string();
    let path = start.next().ok_or("missing request path")?.to_string();
    if start.next() != Some("HTTP/1.1") || start.next().is_some() {
        return Err("expected HTTP/1.1");
    }
    let (mut host, mut origin, mut content_type, mut length) = (None, None, None, None);
    for line in lines {
        let (key, value) = line.split_once(':').ok_or("invalid request header")?;
        let value = value.trim();
        let field = if key.eq_ignore_ascii_case("host") { &mut host }
            else if key.eq_ignore_ascii_case("origin") { &mut origin }
            else if key.eq_ignore_ascii_case("content-type") { &mut content_type }
            else if key.eq_ignore_ascii_case("content-length") { &mut length }
            else if key.eq_ignore_ascii_case("transfer-encoding") {
                return Err("transfer encoding is not supported");
            } else { continue };
        if field.replace(value).is_some() {
            return Err("duplicate request header");
        }
    }
    if host != Some(authority) {
        return Err("unexpected editor host");
    }
    if method == "POST" && (origin != Some(format!("http://{authority}").as_str())
        || content_type != Some("application/json") || length.is_none()) {
        return Err("saves require same-origin JSON requests");
    }
    let want = length.unwrap_or("0").parse::<usize>().map_err(|_| "invalid content length")?;
    if want > MAX_BODY_BYTES {
        return Err("request body too large");
    }
    if body.len() > want {
        return Err("unexpected bytes after request body");
    }
    input.take((want - body.len()) as u64).read_to_end(&mut body)
        .map_err(|_| "incomplete request body")?;
    if body.len() != want {
        return Err("incomplete request body");
    }
    Ok(Request { method, path, body })
}

fn respond(s: &mut impl Write, status: &str, ctype: &str, payload: &[u8]) {
    let _ = write!(
        s,
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        payload.len()
    );
    let _ = s.write_all(payload);
}

/// Save reviewable source edits and regenerate assets. Device control is separate.
fn save(body: &[u8]) -> Result<String, String> {
    let v: serde_json::Value =
        serde_json::from_slice(body).map_err(|e| format!("bad request: {e}"))?;
    let changes = v.as_array().ok_or("bad request: expected an array")?;
    if changes.is_empty() {
        // Notes are already saved; mark this batch complete for optional watchers.
        signal_finish()?;
        return Ok("No coordinate changes. Notes saved in tools/diagram-notes.md; finish signal updated.".into());
    }

    let mut log = save_sources(changes, source_files())?;
    run(&mut log, "regenerate SVGs", "cargo", &[
        "test", "-p", "razer-proto", "--lib", "tests::regenerate_diagram_svgs", "--", "--ignored", "--exact",
    ])?;
    log.push_str("\nSaved source and regenerated SVG assets. Review the diff before building or launching a preview.\n");
    signal_finish()?;
    Ok(log)
}

fn save_sources(changes: &[serde_json::Value], paths: Vec<std::path::PathBuf>) -> Result<String, String> {
    let mut sources: Vec<(std::path::PathBuf, String, bool)> = Vec::new();
    for path in paths {
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("read {}: {e}", path.display()))?;
        sources.push((path, text, false));
    }
    let mut log = String::new();

    // All splices succeed in memory before anything touches disk.
    for ch in changes {
        let kind = ch["kind"].as_str().ok_or("bad change: missing kind")?;
        let nums = |k: &str| -> Result<Vec<i64>, String> {
            ch[k].as_array()
                .ok_or_else(|| format!("bad change: missing {k}"))?
                .iter()
                .map(|n| n.as_i64().ok_or_else(|| "non-integer coordinate".to_string()))
                .collect()
        };
        let (old, new) = (nums("old")?, nums("new")?);
        let same_count = old.len() == new.len();
        let matches: Vec<usize> = sources.iter().enumerate().flat_map(|(index, (_, text, _))| {
            find_spans(text, kind, &old).into_iter().map(move |_| index)
        }).collect();
        let index = match matches.as_slice() {
            [index] => *index,
            [] => return Err(format!(
                "could not find Shape::{kind} with the expected coordinates in any \
                 razer-proto source — it may have changed since this tab loaded; \
                 press reset and retry"
            )),
            _ => return Err(format!(
                "ambiguous Shape::{kind}: the expected coordinates match {} literals; no files written",
                matches.len()
            )),
        };
        let (path, text, dirty) = &mut sources[index];
        let next = if same_count {
            splice(text, kind, &old, &new)
        } else {
            rebuild(text, kind, &old, &new)
        }.ok_or_else(|| format!("invalid coordinates for Shape::{kind}; no files written"))?;
        *text = next;
        *dirty = true;
        let file = path.display().to_string();
        if same_count {
            log.push_str(&format!("spliced Shape::{kind} ({} coordinates) in {file}\n", new.len()));
        } else {
            log.push_str(&format!(
                "rewrote Shape::{kind} ({} → {} coordinates; its comments were dropped) in {file}\n",
                old.len(), new.len()
            ));
        }
    }
    for (path, text, dirty) in &sources {
        if *dirty {
            std::fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))?;
            log.push_str(&format!("wrote {}\n", path.display()));
        }
    }
    Ok(log)
}

/// Mark the end of an edit batch for optional file watchers.
fn signal_finish() -> Result<(), String> {
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    std::fs::write("tools/diagram-finish.signal", format!("{epoch}\n"))
        .map_err(|e| format!("write finish signal: {e}"))
}

/// Append a user annotation to tools/diagram-notes.md — the hand-off channel
/// for reviewing notes about selected shapes and points.
fn note(body: &[u8], device: &str) -> Result<(), String> {
    let v: serde_json::Value =
        serde_json::from_slice(body).map_err(|e| format!("bad request: {e}"))?;
    let text = v["note"].as_str().ok_or("bad request: missing note")?;
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut entry = format!("\n## {device}\n- time: {epoch}\n");
    // multi-selection format: shapes: [{ shape, segments: [..], original? }]
    if let Some(shapes) = v["shapes"].as_array() {
        for s in shapes {
            if let Some(shape) = s["shape"].as_str() {
                entry.push_str(&format!("- shape: `{shape}`\n"));
            }
            if let Some(orig) = s["original"].as_str() {
                entry.push_str(&format!("  - unsaved edit — original source: `{orig}`\n"));
            }
            if let Some(segs) = s["segments"].as_array() {
                for seg in segs.iter().filter_map(|x| x.as_str()) {
                    entry.push_str(&format!("  - selected {seg}\n"));
                }
            }
        }
    }
    // single-shape format kept for compatibility
    if let Some(shape) = v["shape"].as_str() {
        entry.push_str(&format!("- shape: `{shape}`\n"));
    }
    if let Some(orig) = v["original"].as_str() {
        entry.push_str(&format!("- unsaved edit — original source: `{orig}`\n"));
    }
    if let Some(pt) = v["point"].as_str() {
        entry.push_str(&format!("- about this specific point: {pt} (last one touched)\n"));
    }
    entry.push_str(&format!("- note: {text}\n"));
    use std::io::Write as _;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("tools/diagram-notes.md")
        .map_err(|e| e.to_string())?;
    f.write_all(entry.as_bytes()).map_err(|e| e.to_string())
}

fn run(log: &mut String, label: &str, cmd: &str, args: &[&str]) -> Result<(), String> {
    log.push_str(&format!("{label}: {cmd} {}\n", args.join(" ")));
    let out = std::process::Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("{log}\n{label} failed to start: {e}"))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // keep the tail — cargo output is long and the end has the verdict
    let tail: String = text.lines().rev().take(12).collect::<Vec<_>>().into_iter().rev()
        .collect::<Vec<_>>().join("\n");
    log.push_str(&tail);
    log.push('\n');
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("{log}\n{label} FAILED — the source keeps your edit; fix or `git checkout` it"))
    }
}

/// Locate every matching literal so duplicate coordinates never select artwork
/// by file order or by the first occurrence within a device source.
fn find_spans(src: &str, kind: &str, old: &[i64]) -> Vec<(usize, usize)> {
    let needle = format!("Shape::{kind} {{");
    let mut from = 0;
    let mut matches = Vec::new();
    while let Some(rel) = src[from..].find(&needle) {
        let start = from + rel;
        let mut depth = 0usize;
        let mut end = None;
        for (i, c) in src[start..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(start + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = end else { return Vec::new() };
        if int_spans(&src[start..end]).iter().map(|s| s.2).eq(old.iter().copied()) {
            matches.push((start, end));
        }
        from = start + needle.len();
    }
    matches
}

fn find_span(src: &str, kind: &str, old: &[i64]) -> Option<(usize, usize)> {
    match find_spans(src, kind, old).as_slice() {
        [span] => Some(*span),
        _ => None,
    }
}

/// Same-count edit: replace only the integers in the matched literal, leaving
/// formatting and comments untouched.
fn splice(src: &str, kind: &str, old: &[i64], new: &[i64]) -> Option<String> {
    let (start, end) = find_span(src, kind, old)?;
    let span = &src[start..end];
    let spans = int_spans(span);
    let mut rebuilt = String::with_capacity(span.len());
    let mut last = 0;
    for ((off, len, _), n) in spans.iter().zip(new) {
        rebuilt.push_str(&span[last..*off]);
        rebuilt.push_str(&n.to_string());
        last = off + len;
    }
    rebuilt.push_str(&span[last..]);
    Some(format!("{}{}{}", &src[..start], rebuilt, &src[end..]))
}

/// Count-changing edit (point deletion): regenerate the whole shape literal
/// from the new coordinates, preserving role/closed and the line's indent.
fn rebuild(src: &str, kind: &str, old: &[i64], new: &[i64]) -> Option<String> {
    let (start, end) = find_span(src, kind, old)?;
    let span = &src[start..end];
    let role = span.split("role: ").nth(1)?.split(',').next()?.trim().to_string();
    let nl = src[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let indent = &src[nl..start];
    if !indent.chars().all(|c| c == ' ' || c == '\t') {
        return None;
    }
    let lit = match kind {
        "Path" => {
            if new.len() < 8 || !(new.len() - 2).is_multiple_of(6) {
                return None;
            }
            let closed = span.contains("closed: true");
            let mut s = format!(
                "Shape::Path {{ role: {role}, start: ({}, {}), closed: {closed}, curves: &[\n",
                new[0], new[1]
            );
            for c in new[2..].chunks(6) {
                s.push_str(&format!(
                    "{indent}    (({}, {}), ({}, {}), ({}, {})),\n",
                    c[0], c[1], c[2], c[3], c[4], c[5]
                ));
            }
            s.push_str(&format!("{indent}]}}"));
            s
        }
        "Polyline" => {
            if new.len() < 4 || !new.len().is_multiple_of(2) {
                return None;
            }
            let pts: Vec<String> =
                new.chunks(2).map(|p| format!("({}, {})", p[0], p[1])).collect();
            format!("Shape::Polyline {{ role: {role}, points: &[{}] }}", pts.join(", "))
        }
        _ => return None, // rects/circles never change coordinate count
    };
    Some(format!("{}{}{}", &src[..start], lit, &src[end..]))
}

/// (offset, length, value) of every decimal integer literal in `s`,
/// including a leading minus sign.
fn int_spans(s: &str) -> Vec<(usize, usize, i64)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() || (b[i] == b'-' && i + 1 < b.len() && b[i + 1].is_ascii_digit()) {
            let start = i;
            if b[i] == b'-' {
                i += 1;
            }
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            if let Ok(v) = s[start..i].parse::<i64>() {
                out.push((start, i - start, v));
            }
        } else {
            i += 1;
        }
    }
    out
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const AUTHORITY: &str = "127.0.0.1:7333";

    fn post(headers: &str, body: &str) -> Vec<u8> {
        format!("POST /save HTTP/1.1\r\nHost: {AUTHORITY}\r\n{headers}\r\n{body}").into_bytes()
    }

    #[test]
    fn artwork_selection_uses_specs_without_opening_hardware() {
        assert_eq!(select_model(Some("deathadder-v3")).unwrap(), razer_proto::DEATHADDER_V3);
        assert_eq!(select_model(Some("DeathAdder Elite")).unwrap(), razer_proto::DEATHADDER_ELITE);
        assert!(select_model(None).unwrap().diagram.is_some());
        for spec in razer_proto::SUPPORTED.iter().filter(|s| s.diagram.is_none()) {
            assert!(select_model(Some(spec.name)).is_none(), "no diagram to edit");
        }
        assert!(select_model(Some("unknown")).is_none());
    }

    #[test]
    fn own_origin_json_request_is_accepted_and_get_needs_no_origin() {
        let headers = format!("Origin: http://{AUTHORITY}\r\nContent-Type: application/json\r\nContent-Length: 2\r\n");
        let request = read_request(&mut Cursor::new(post(&headers, "[]")), AUTHORITY).unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/save");
        assert_eq!(request.body, b"[]");
        let get = format!("GET /diagram.svg?ts=1 HTTP/1.1\r\nHost: {AUTHORITY}\r\n\r\n");
        assert_eq!(read_request(&mut Cursor::new(get), AUTHORITY).unwrap().path, "/diagram.svg?ts=1");
    }

    #[test]
    fn mutation_rejects_foreign_or_missing_origin_and_form_bodies() {
        for origin in ["", "Origin: null\r\n", "Origin: https://example.com\r\n",
            "Origin: http://127.0.0.1:7334\r\n"] {
            let headers = format!("{origin}Content-Type: application/json\r\nContent-Length: 2\r\n");
            assert_eq!(read_request(&mut Cursor::new(post(&headers, "[]")), AUTHORITY).unwrap_err(),
                "saves require same-origin JSON requests");
        }
        for content_type in ["text/plain", "application/x-www-form-urlencoded"] {
            let headers = format!("Origin: http://{AUTHORITY}\r\nContent-Type: {content_type}\r\nContent-Length: 2\r\n");
            assert!(read_request(&mut Cursor::new(post(&headers, "[]")), AUTHORITY).is_err());
        }
        let rebinding = b"GET / HTTP/1.1\r\nHost: attacker.example:7333\r\n\r\n";
        assert_eq!(read_request(&mut Cursor::new(rebinding), AUTHORITY).unwrap_err(), "unexpected editor host");
    }

    #[test]
    fn ambiguous_truncated_and_oversized_requests_are_rejected() {
        let base = format!("Origin: http://{AUTHORITY}\r\nContent-Type: application/json\r\n");
        for extra in ["Content-Length: 2\r\nContent-Length: 2\r\n",
            "Content-Length: 2\r\nTransfer-Encoding: chunked\r\n",
            "Content-Length: invalid\r\n", ""] {
            assert!(read_request(&mut Cursor::new(post(&(base.clone() + extra), "[]")), AUTHORITY).is_err());
        }
        let too_large = format!("{base}Content-Length: {}\r\n", MAX_BODY_BYTES + 1);
        assert_eq!(read_request(&mut Cursor::new(post(&too_large, "")), AUTHORITY).unwrap_err(), "request body too large");
        let headers = format!("{base}Content-Length: 3\r\n");
        assert_eq!(read_request(&mut Cursor::new(post(&headers, "[]")), AUTHORITY).unwrap_err(), "incomplete request body");
        let headers = format!("{base}Content-Length: 2\r\n");
        assert_eq!(read_request(&mut Cursor::new(post(&headers, "[]extra")), AUTHORITY).unwrap_err(), "unexpected bytes after request body");
        assert_eq!(read_request(&mut Cursor::new(vec![b'x'; MAX_HEADER_BYTES + 1]), AUTHORITY).unwrap_err(), "request headers too large");
    }

    #[test]
    fn invalid_mutation_is_rejected_before_reading_its_body() {
        struct HeaderOnly(Option<Vec<u8>>);
        impl Read for HeaderOnly {
            fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
                let head = self.0.take().expect("rejected origin must not read a body");
                bytes[..head.len()].copy_from_slice(&head);
                Ok(head.len())
            }
        }
        let headers = "Origin: https://example.com\r\nContent-Type: application/json\r\nContent-Length: 100\r\n";
        assert!(read_request(&mut HeaderOnly(Some(post(headers, ""))), AUTHORITY).is_err());
    }

    struct TestSource(std::path::PathBuf);

    impl TestSource {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!("snakecharmer-editor-{}-{stamp}-{sequence}", std::process::id()));
            std::fs::create_dir(&root).unwrap();
            let path = root.join("device.rs");
            std::fs::write(&path, SRC).unwrap();
            Self(path)
        }
    }

    impl Drop for TestSource {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.0.parent().unwrap());
        }
    }

    #[test]
    fn stale_source_rejects_the_entire_batch_before_any_write() {
        let source = TestSource::new();
        let changes = serde_json::json!([
            { "kind": "Polyline", "old": [1, 1, 2, 2, 3, 3], "new": [1, 2, 2, 2, 3, 3] },
            { "kind": "Circle", "old": [9, 9, 9], "new": [10, 10, 10] }
        ]);
        assert!(save_sources(changes.as_array().unwrap(), vec![source.0.clone()]).is_err());
        assert_eq!(std::fs::read_to_string(&source.0).unwrap(), SRC);
    }

    #[test]
    fn duplicate_literals_within_a_file_reject_the_batch_before_any_write() {
        let source = TestSource::new();
        let duplicate = "\nShape::Circle { role: Role::Detail, cx: 9, cy: 9, r: 9 }";
        let original = format!("{SRC}{duplicate}{duplicate}");
        std::fs::write(&source.0, &original).unwrap();
        let changes = serde_json::json!([
            { "kind": "Polyline", "old": [1, 1, 2, 2, 3, 3], "new": [1, 2, 2, 2, 3, 3] },
            { "kind": "Circle", "old": [9, 9, 9], "new": [10, 10, 10] }
        ]);
        let error = save_sources(changes.as_array().unwrap(), vec![source.0.clone()]).unwrap_err();
        assert!(error.contains("ambiguous Shape::Circle"));
        assert_eq!(std::fs::read_to_string(&source.0).unwrap(), original);
        assert!(splice(&original, "Circle", &[9, 9, 9], &[10, 10, 10]).is_none());
    }

    #[test]
    fn identical_literals_in_different_files_never_choose_the_first_device() {
        let first = TestSource::new();
        let second = TestSource::new();
        let changes = serde_json::json!([
            { "kind": "Polyline", "old": [1, 1, 2, 2, 3, 3], "new": [1, 2, 2, 2, 3, 3] }
        ]);
        let error = save_sources(changes.as_array().unwrap(), vec![first.0.clone(), second.0.clone()]).unwrap_err();
        assert!(error.contains("ambiguous Shape::Polyline"));
        assert_eq!(std::fs::read_to_string(&first.0).unwrap(), SRC);
        assert_eq!(std::fs::read_to_string(&second.0).unwrap(), SRC);
    }

    #[test]
    fn saving_coordinates_produces_a_source_diff_and_preserves_neighbours() {
        let source = TestSource::new();
        let changes = serde_json::json!([
            { "kind": "Polyline", "old": [1, 1, 2, 2, 3, 3], "new": [1, 2, 2, 2, 3, 3] }
        ]);
        let log = save_sources(changes.as_array().unwrap(), vec![source.0.clone()]).unwrap();
        assert!(log.contains("wrote"));
        assert_eq!(std::fs::read_to_string(&source.0).unwrap(),
            SRC.replace("points: &[(1, 1), (2, 2), (3, 3)]", "points: &[(1, 2), (2, 2), (3, 3)]"));
    }

    const SRC: &str = r#"
        vec![
            Shape::Path { role: Role::Body, start: (10, 20), closed: true, curves: &[
                ((1, 2), (3, 4), (5, 6)),    // first
                ((7, 8), (9, 10), (10, 20)), // second (closes)
            ]},
            Shape::Polyline { role: Role::Detail, points: &[(1, 1), (2, 2), (3, 3)] },
        ]
"#;

    #[test]
    fn rebuild_path_drops_a_segment() {
        let old = [10, 20, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 10, 20];
        let new = [10, 20, 1, 2, 9, 10, 10, 20]; // merged into one curve
        let out = rebuild(SRC, "Path", &old, &new).expect("rebuild");
        assert!(out.contains(
            "Shape::Path { role: Role::Body, start: (10, 20), closed: true, curves: &[\n\
             \x20               ((1, 2), (9, 10), (10, 20)),\n\
             \x20           ]}"
        ), "got:\n{out}");
        assert!(out.contains("Shape::Polyline")); // neighbour untouched
        assert!(!out.contains("// first")); // its comments are gone, by design
    }

    #[test]
    fn rebuild_polyline_drops_a_vertex() {
        let out = rebuild(SRC, "Polyline", &[1, 1, 2, 2, 3, 3], &[1, 1, 3, 3]).expect("rebuild");
        assert!(out.contains("Shape::Polyline { role: Role::Detail, points: &[(1, 1), (3, 3)] }"));
    }

    #[test]
    fn rebuild_rejects_wrong_originals() {
        assert!(rebuild(SRC, "Polyline", &[9, 9, 9, 9], &[1, 1, 3, 3]).is_none());
    }

    #[test]
    fn splice_still_preserves_comments() {
        let old = [10, 20, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 10, 20];
        let new = [10, 20, 1, 2, 3, 4, 5, 7, 7, 8, 9, 10, 10, 20];
        let out = splice(SRC, "Path", &old, &new).expect("splice");
        assert!(out.contains("(5, 7)),    // first"));
    }
}
