//! Local lemma-editing web server (`morpheus edit`).
//!
//! Serves a single-page UI (edit.html) plus a small JSON API on
//! 127.0.0.1. Edits are written to an overlay directory in stemlib source
//! format (beta-code); the upstream stemlib is never modified.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use serde_json::{json, Value};
use tiny_http::{Header, Method, Response, Server};

use crate::error::{MorpheusError, Result};
use crate::generate::{form_to_json, generate_for_entry, GenerateOptions};
use crate::stemlib::conjsys::expand_one_entry;
use crate::stemlib::stem_dict::{parse_stem_content, StemEntry, StemKind};
use crate::stemlib::{Language, StemlibIndex};
use crate::unicode::betacode::{beta_to_unicode, unicode_to_beta};

use super::raw_index::LemmaSourceIndex;

static EDIT_HTML: &str = include_str!("edit.html");

struct State {
    morphlib: PathBuf,
    overlay:  PathBuf,
    language: Language,
    stemlib:  RwLock<Arc<StemlibIndex>>,
    sources:  RwLock<Arc<LemmaSourceIndex>>,
}

pub fn run_edit_server(
    morphlib: PathBuf,
    overlay: PathBuf,
    language: Language,
    port: u16,
) -> Result<()> {
    std::fs::create_dir_all(overlay.join(language.dir_name()).join("stemsrc"))
        .map_err(MorpheusError::Io)?;

    let overlays = vec![overlay.clone()];
    let stemlib = StemlibIndex::load_with_overlays(&morphlib, language, &overlays)?;
    let sources = LemmaSourceIndex::build(&morphlib, &overlays, language);

    let state = State {
        morphlib,
        overlay,
        language,
        stemlib: RwLock::new(Arc::new(stemlib)),
        sources: RwLock::new(Arc::new(sources)),
    };

    let addr = format!("127.0.0.1:{port}");
    let server = Server::http(&addr)
        .map_err(|e| MorpheusError::StemlibLoad(format!("cannot bind {addr}: {e}")))?;
    println!("morpheus edit server listening on http://{addr}/");
    println!("overlay directory: {}", state.overlay.display());

    for mut request in server.incoming_requests() {
        let url = request.url().to_string();
        let (path, query) = match url.split_once('?') {
            Some((p, q)) => (p.to_string(), q.to_string()),
            None => (url.clone(), String::new()),
        };

        let response = match (request.method(), path.as_str()) {
            (Method::Get, "/") => html_response(EDIT_HTML),
            (Method::Get, "/api/search") => api_search(&state, &query),
            (Method::Get, "/api/lemma") => api_lemma(&state, &query),
            (Method::Get, "/api/types") => api_types(&state),
            (Method::Post, "/api/preview") => {
                let body = read_body(&mut request);
                api_preview(&state, &body)
            }
            (Method::Post, "/api/save") => {
                let body = read_body(&mut request);
                api_save(&state, &body)
            }
            (Method::Post, "/api/delete") => {
                let body = read_body(&mut request);
                api_delete(&state, &body)
            }
            _ => json_response(404, &json!({"error": "not found"})),
        };
        let _ = request.respond(response);
    }
    Ok(())
}

// ── HTTP helpers ────────────────────────────────────────────────────────────

fn read_body(request: &mut tiny_http::Request) -> String {
    let mut body = String::new();
    let _ = request.as_reader().read_to_string(&mut body);
    body
}

fn html_response(body: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(body).with_header(
        Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap(),
    )
}

fn json_response(status: u16, value: &Value) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(value.to_string())
        .with_status_code(status)
        .with_header(Header::from_bytes("Content-Type", "application/json").unwrap())
}

/// Extract and percent-decode one query parameter.
fn query_param(query: &str, name: &str) -> Option<String> {
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=')?;
        if k == name {
            return Some(percent_decode(v));
        }
    }
    None
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                if let (Some(h), Some(l)) = (
                    bytes.get(i + 1).and_then(|b| (*b as char).to_digit(16)),
                    bytes.get(i + 2).and_then(|b| (*b as char).to_digit(16)),
                ) {
                    out.push((h * 16 + l) as u8);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ── API endpoints ───────────────────────────────────────────────────────────

fn api_search(state: &State, query: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let q = query_param(query, "q").unwrap_or_default();
    // ASCII input is treated as beta-code (lo/gos) as well as plain text.
    let q_uni = if q.is_ascii() && !q.is_empty() {
        beta_to_unicode(&q)
    } else {
        q.clone()
    };
    let sources = state.sources.read().unwrap().clone();
    let mut hits = sources.search(&q_uni, 50);
    if hits.is_empty() && q_uni != q {
        hits = sources.search(&q, 50);
    }
    json_response(200, &json!({ "lemmas": hits }))
}

fn api_lemma(state: &State, query: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let Some(lemma) = query_param(query, "l") else {
        return json_response(400, &json!({"error": "missing l parameter"}));
    };
    let sources = state.sources.read().unwrap().clone();
    let blocks: Vec<Value> = sources
        .by_lemma
        .get(&lemma)
        .map(|blocks| {
            blocks
                .iter()
                .map(|b| {
                    json!({
                        "file": b.file.file_name().and_then(|n| n.to_str()).unwrap_or(""),
                        "line": b.line,
                        "raw": b.raw,
                        "unicode": block_to_unicode(&b.raw),
                        "is_overlay": b.is_overlay,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    json_response(200, &json!({ "lemma": lemma, "blocks": blocks }))
}

fn api_types(state: &State) -> Response<std::io::Cursor<Vec<u8>>> {
    let stemlib = state.stemlib.read().unwrap().clone();
    let mut stemtypes: Vec<Value> = stemlib
        .stem_types
        .values()
        .map(|s| json!({"name": s.name, "class": s.class_str}))
        .collect();
    stemtypes.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    let mut derivtypes: Vec<Value> = stemlib
        .deriv_types
        .values()
        .map(|s| json!({"name": s.name, "class": s.class_str}))
        .collect();
    derivtypes.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    json_response(200, &json!({ "stemtypes": stemtypes, "derivtypes": derivtypes }))
}

fn api_preview(state: &State, body: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let Ok(req) = serde_json::from_str::<Value>(body) else {
        return json_response(400, &json!({"error": "invalid JSON body"}));
    };
    let Some(block) = req["block"].as_str() else {
        return json_response(400, &json!({"error": "missing block"}));
    };

    let beta_block = block_to_beta(block);
    let entries = parse_block_entries(&beta_block);
    if entries.is_empty() {
        return json_response(
            200,
            &json!({"forms": [], "beta": beta_block,
                    "error": "no parseable :le: + stem lines"}),
        );
    }

    let stemlib = state.stemlib.read().unwrap().clone();
    let expanded = expand_block_entries(&entries, &stemlib);
    let opts = GenerateOptions::default();
    let mut forms = Vec::new();
    for entry in &expanded {
        for f in generate_for_entry(entry, &stemlib, &opts) {
            forms.push(form_to_json(&f));
        }
    }
    json_response(
        200,
        &json!({"forms": forms, "beta": beta_block, "entries": entries.len()}),
    )
}

fn api_save(state: &State, body: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let Ok(req) = serde_json::from_str::<Value>(body) else {
        return json_response(400, &json!({"error": "invalid JSON body"}));
    };
    let Some(block) = req["block"].as_str() else {
        return json_response(400, &json!({"error": "missing block"}));
    };

    let beta_block = block_to_beta(block);
    let entries = parse_block_entries(&beta_block);
    if entries.is_empty() {
        return json_response(
            400,
            &json!({"error": "block does not parse (need a :le: line plus at least one stem line)"}),
        );
    }
    let lemma = entries[0].lemma.clone();

    // Validate: the block must yield at least one generated form.
    {
        let stemlib = state.stemlib.read().unwrap().clone();
        let expanded = expand_block_entries(&entries, &stemlib);
        let opts = GenerateOptions::default();
        let n_forms: usize = expanded
            .iter()
            .map(|e| generate_for_entry(e, &stemlib, &opts).len())
            .sum();
        if n_forms == 0 {
            return json_response(
                400,
                &json!({"error": "entry generates no forms — check the stemtype"}),
            );
        }
    }

    // Verbal blocks go to custom.vbs, nominal ones to custom.nom.
    let verbal = entries
        .iter()
        .any(|e| matches!(e.kind, StemKind::Verb | StemKind::Deriv));
    let file_name = if verbal { "custom.vbs" } else { "custom.nom" };
    let target = state
        .overlay
        .join(state.language.dir_name())
        .join("stemsrc")
        .join(file_name);

    if let Err(e) = upsert_overlay_block(&target, &lemma, &beta_block) {
        return json_response(500, &json!({"error": format!("write failed: {e}")}));
    }
    if let Err(e) = reload(state) {
        return json_response(500, &json!({"error": format!("reload failed: {e}")}));
    }
    json_response(
        200,
        &json!({"saved": lemma, "file": file_name, "beta": beta_block}),
    )
}

fn api_delete(state: &State, body: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let Ok(req) = serde_json::from_str::<Value>(body) else {
        return json_response(400, &json!({"error": "invalid JSON body"}));
    };
    let Some(lemma) = req["lemma"].as_str() else {
        return json_response(400, &json!({"error": "missing lemma"}));
    };

    let sources = state.sources.read().unwrap().clone();
    let overlay_blocks: Vec<_> = sources
        .by_lemma
        .get(lemma)
        .map(|bs| bs.iter().filter(|b| b.is_overlay).collect())
        .unwrap_or_default();
    if overlay_blocks.is_empty() {
        return json_response(
            400,
            &json!({"error": "lemma has no overlay entries (upstream entries are read-only)"}),
        );
    }
    for block in &overlay_blocks {
        if let Err(e) = remove_overlay_block(&block.file, lemma) {
            return json_response(500, &json!({"error": format!("delete failed: {e}")}));
        }
    }
    if let Err(e) = reload(state) {
        return json_response(500, &json!({"error": format!("reload failed: {e}")}));
    }
    json_response(200, &json!({"deleted": lemma}))
}

fn reload(state: &State) -> Result<()> {
    let overlays = vec![state.overlay.clone()];
    let stemlib =
        StemlibIndex::load_with_overlays(&state.morphlib, state.language, &overlays)?;
    let sources = LemmaSourceIndex::build(&state.morphlib, &overlays, state.language);
    *state.stemlib.write().unwrap() = Arc::new(stemlib);
    *state.sources.write().unwrap() = Arc::new(sources);
    Ok(())
}

// ── Block handling ──────────────────────────────────────────────────────────

const TAGS: &[&str] = &[":le:", ":no:", ":aj:", ":vs:", ":vb:", ":de:", ":wd:", ":wk:"];

/// Convert a user-entered block to beta-code: the lemma/stem token of each
/// tagged line is converted (Unicode input allowed); key tokens stay ASCII.
fn block_to_beta(block: &str) -> String {
    block
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            let (dash, rest) = match trimmed.strip_prefix('-') {
                Some(r) => ("-", r),
                None => ("", trimmed),
            };
            for tag in TAGS {
                if let Some(payload) = rest.strip_prefix(tag) {
                    let payload = payload.trim();
                    let (word, keys) = match payload.split_once(char::is_whitespace) {
                        Some((w, k)) => (w, k.trim()),
                        None => (payload, ""),
                    };
                    let word_beta = if word.is_ascii() {
                        word.to_string()
                    } else {
                        unicode_to_beta(word)
                    };
                    return if keys.is_empty() {
                        format!("{dash}{tag}{word_beta}")
                    } else {
                        format!("{dash}{tag}{word_beta} {keys}")
                    };
                }
            }
            trimmed.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render a beta-code block for display: lemma/stem tokens in Unicode.
fn block_to_unicode(block: &str) -> String {
    block
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            let (dash, rest) = match trimmed.strip_prefix('-') {
                Some(r) => ("-", r),
                None => ("", trimmed),
            };
            for tag in TAGS {
                if let Some(payload) = rest.strip_prefix(tag) {
                    let payload = payload.trim();
                    let (word, keys) = match payload.split_once(char::is_whitespace) {
                        Some((w, k)) => (w, k.trim()),
                        None => (payload, ""),
                    };
                    let word_uni = beta_to_unicode(word);
                    return if keys.is_empty() {
                        format!("{dash}{tag}{word_uni}")
                    } else {
                        format!("{dash}{tag}{word_uni} {keys}")
                    };
                }
            }
            trimmed.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_block_entries(beta_block: &str) -> Vec<StemEntry> {
    parse_stem_content(beta_block)
}

/// The entries of a block plus their conjsys expansions (so :de: entries
/// preview their full principal-part paradigm).
fn expand_block_entries(entries: &[StemEntry], stemlib: &StemlibIndex) -> Vec<StemEntry> {
    let mut out: Vec<StemEntry> = Vec::new();
    for entry in entries {
        if entry.kind == StemKind::Deriv {
            out.extend(expand_one_entry(entry, &stemlib.deriv_tables));
        }
        out.push(entry.clone());
    }
    out
}

/// Insert or replace the block for `lemma` in an overlay file.
fn upsert_overlay_block(path: &Path, lemma: &str, beta_block: &str) -> std::io::Result<()> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let mut out = String::new();
    let mut replaced = false;
    for block in split_blocks(&existing) {
        let block_lemma = block
            .lines()
            .next()
            .and_then(|l| l.trim().strip_prefix(":le:"))
            .map(|b| crate::stemlib::stem_dict::beta_to_unicode_word(b.trim()))
            .unwrap_or_default();
        if block_lemma == lemma {
            if !replaced {
                out.push_str(beta_block);
                out.push_str("\n\n");
                replaced = true;
            }
            // duplicate blocks for the lemma are dropped
        } else {
            out.push_str(&block);
            out.push_str("\n\n");
        }
    }
    if !replaced {
        out.push_str(beta_block);
        out.push_str("\n\n");
    }
    std::fs::write(path, out)
}

fn remove_overlay_block(path: &Path, lemma: &str) -> std::io::Result<()> {
    let existing = std::fs::read_to_string(path)?;
    let mut out = String::new();
    for block in split_blocks(&existing) {
        let block_lemma = block
            .lines()
            .next()
            .and_then(|l| l.trim().strip_prefix(":le:"))
            .map(|b| crate::stemlib::stem_dict::beta_to_unicode_word(b.trim()))
            .unwrap_or_default();
        if block_lemma != lemma {
            out.push_str(&block);
            out.push_str("\n\n");
        }
    }
    std::fs::write(path, out)
}

/// Split a stem file into `:le:`-headed blocks (preamble lines are dropped).
fn split_blocks(content: &str) -> Vec<String> {
    let mut blocks: Vec<Vec<&str>> = Vec::new();
    for line in content.lines() {
        if line.trim().starts_with(":le:") {
            blocks.push(vec![line]);
        } else if let Some(current) = blocks.last_mut() {
            if !line.trim().is_empty() {
                current.push(line);
            }
        }
    }
    blocks.into_iter().map(|b| b.join("\n")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_block_converts_to_beta() {
        let block = ":le:σοφία\n:no:σοφι a_hs fem";
        let beta = block_to_beta(block);
        assert_eq!(beta, ":le:sofi/a\n:no:sofi a_hs fem");
        // Round-trip back to display form
        assert_eq!(block_to_unicode(&beta), ":le:σοφία\n:no:σοφι a_hs fem");
    }

    #[test]
    fn beta_block_passes_through() {
        let block = ":le:sofi/a\n:no:sofi a_hs fem";
        assert_eq!(block_to_beta(block), block);
    }

    #[test]
    fn upsert_and_remove_overlay_block() {
        let dir = std::env::temp_dir().join(format!("morpheus-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("custom.nom");

        upsert_overlay_block(&file, "σοφία", ":le:sofi/a\n:no:sofi a_hs fem").unwrap();
        upsert_overlay_block(&file, "λόγος", ":le:lo/gos\n:no:log os_ou masc").unwrap();
        // Replace the first lemma
        upsert_overlay_block(&file, "σοφία", ":le:sofi/a\n:no:sofi h_hs fem").unwrap();

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content.matches(":le:sofi/a").count(), 1);
        assert!(content.contains("h_hs"));
        assert!(!content.contains("a_hs"));
        assert!(content.contains(":le:lo/gos"));

        remove_overlay_block(&file, "σοφία").unwrap();
        let content = std::fs::read_to_string(&file).unwrap();
        assert!(!content.contains("sofi"));
        assert!(content.contains(":le:lo/gos"));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
