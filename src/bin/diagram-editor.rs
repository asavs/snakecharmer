//! diagram-editor — a dev-only local server for visually editing device
//! diagrams. Detects the plugged-in mouse, serves its generated SVG with
//! draggable points in the browser, and on Save writes the new coordinates
//! back into `crates/razer-proto/src/lib.rs`, regenerates the SVG assets,
//! rebuilds the release daemon, and restarts it.
//!
//! Run from the repo root:  cargo run --bin diagram-editor
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
    let name = match razer_hid::Mouse::open() {
        Ok(m) => {
            let n = m.spec().name.to_string();
            println!("device: {n} (detected)");
            n
        }
        Err(e) => {
            let n = razer_proto::SUPPORTED[0].name.to_string();
            println!("no device detected ({e}); editing {n}");
            n
        }
    };
    let slug = name.to_lowercase().replace(' ', "-");

    let listener = TcpListener::bind("127.0.0.1:7333")
        .or_else(|_| TcpListener::bind("127.0.0.1:0"))
        .expect("bind");
    let url = format!("http://{}", listener.local_addr().unwrap());
    println!("editor: {url}  (Ctrl+C to quit)");
    let _ = std::process::Command::new("cmd").args(["/C", "start", "", &url]).spawn();

    for stream in listener.incoming().flatten() {
        handle(stream, &name, &slug);
    }
}

fn handle(mut s: TcpStream, name: &str, slug: &str) {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    let (head, mut body) = loop {
        match s.read(&mut tmp) {
            Ok(0) | Err(_) => return,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
        }
        if let Some(pos) = find(&buf, b"\r\n\r\n") {
            break (String::from_utf8_lossy(&buf[..pos]).to_string(), buf[pos + 4..].to_vec());
        }
        if buf.len() > 1 << 20 {
            return;
        }
    };
    let want = content_length(&head);
    while body.len() < want {
        match s.read(&mut tmp) {
            Ok(0) | Err(_) => break,
            Ok(n) => body.extend_from_slice(&tmp[..n]),
        }
    }

    let line = head.lines().next().unwrap_or("");
    let (status, ctype, payload): (&str, &str, Vec<u8>) = if line.starts_with("GET / ") {
        ("200 OK", "text/html; charset=utf-8", HTML.as_bytes().to_vec())
    } else if line.starts_with("GET /device") {
        ("200 OK", "text/plain; charset=utf-8", name.as_bytes().to_vec())
    } else if line.starts_with("GET /notes") {
        let text = std::fs::read_to_string("tools/diagram-notes.md").unwrap_or_default();
        ("200 OK", "text/plain; charset=utf-8", text.into_bytes())
    } else if line.starts_with("GET /diagram.svg") {
        match std::fs::read(format!("docs/assets/{slug}.svg")) {
            Ok(svg) => ("200 OK", "image/svg+xml", svg),
            Err(e) => ("500 Internal Server Error", "text/plain", e.to_string().into_bytes()),
        }
    } else if line.starts_with("POST /save") {
        match save(&body) {
            Ok(log) => ("200 OK", "text/plain; charset=utf-8", log.into_bytes()),
            Err(log) => ("409 Conflict", "text/plain; charset=utf-8", log.into_bytes()),
        }
    } else if line.starts_with("POST /note") {
        match note(&body, name) {
            Ok(()) => ("200 OK", "text/plain; charset=utf-8", b"ok".to_vec()),
            Err(e) => ("500 Internal Server Error", "text/plain; charset=utf-8", e.into_bytes()),
        }
    } else {
        ("404 Not Found", "text/plain", b"not found".to_vec())
    };

    let _ = write!(
        s,
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        payload.len()
    );
    let _ = s.write_all(&payload);
}

/// Splice edited coordinates into lib.rs, then run the full pipeline:
/// regenerate SVG assets, rebuild release, restart the daemon.
fn save(body: &[u8]) -> Result<String, String> {
    let v: serde_json::Value =
        serde_json::from_slice(body).map_err(|e| format!("bad request: {e}"))?;
    let changes = v.as_array().ok_or("bad request: expected an array")?;
    if changes.is_empty() {
        // notes-only finish: nothing to write, just wake the agent
        signal_finish()?;
        return Ok("no coordinate changes — finished; claude will pick up your notes".into());
    }

    let mut sources: Vec<(std::path::PathBuf, String, bool)> = Vec::new();
    for path in source_files() {
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
        let hit = sources.iter_mut().find_map(|(path, text, dirty)| {
            let next = if same_count {
                splice(text, kind, &old, &new)
            } else {
                rebuild(text, kind, &old, &new)
            }?;
            *text = next;
            *dirty = true;
            Some(path.display().to_string())
        });
        let file = hit.ok_or_else(|| {
            format!(
                "could not find Shape::{kind} with the expected coordinates in any \
                 razer-proto source — it may have changed since this tab loaded; \
                 press reset and retry"
            )
        })?;
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
    log.push('\n');

    run(&mut log, "regenerate SVGs", "cargo",
        &["test", "-p", "razer-proto", "--lib", "--", "--ignored"])?;
    log.push_str("stopping daemon…\n");
    let _ = std::process::Command::new("taskkill").args(["/IM", "snakecharmer.exe", "/F"]).output();
    std::thread::sleep(std::time::Duration::from_millis(400));
    // only the daemon: a bare `--release` would also try to relink this very
    // server, whose exe is locked while it runs, failing the whole save
    run(&mut log, "rebuild release", "cargo", &["build", "--release", "--bin", "snakecharmer"])?;
    match std::process::Command::new("target\\release\\snakecharmer.exe").spawn() {
        Ok(_) => log.push_str("daemon restarted\n\nsaved. the diagram below is the regenerated one."),
        Err(e) => log.push_str(&format!("daemon relaunch FAILED: {e}\n")),
    }
    signal_finish()?;
    Ok(log)
}

/// Touch the file the agent's watcher waits on — notes queue silently until
/// the user presses "finish".
fn signal_finish() -> Result<(), String> {
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    std::fs::write("tools/diagram-finish.signal", format!("{epoch}\n"))
        .map_err(|e| format!("write finish signal: {e}"))
}

/// Append a user annotation to tools/diagram-notes.md — the hand-off channel
/// for "click a shape in the editor, then talk to Claude about it".
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
                entry.push_str(&format!("  - unsaved edit — original in lib.rs: `{orig}`\n"));
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
        entry.push_str(&format!("- unsaved edit — original in lib.rs: `{orig}`\n"));
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

/// Locate the first `Shape::<kind> { … }` literal whose integer sequence
/// equals `old`, returning its byte span in `src`.
fn find_span(src: &str, kind: &str, old: &[i64]) -> Option<(usize, usize)> {
    let needle = format!("Shape::{kind} {{");
    let mut from = 0;
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
        let end = end?;
        if int_spans(&src[start..end]).iter().map(|s| s.2).eq(old.iter().copied()) {
            return Some((start, end));
        }
        from = start + needle.len();
    }
    None
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

fn content_length(head: &str) -> usize {
    head.lines()
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim().eq_ignore_ascii_case("content-length").then(|| v.trim().parse().ok())?
        })
        .unwrap_or(0)
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

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
