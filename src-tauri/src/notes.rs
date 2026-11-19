// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// Additional permission under GNU GPL version 3 section 7:
// see LICENSE-EXCEPTION.md (EON Module ABI Exception 1.0).

//! Notes, kept in the EON Office core.
//!
//! A note is a document in the core's `notes` collection:
//! `{ title, body (Markdown), notebook, tags, created, updated }`. Links are
//! `[[Title]]` or `[[Title|shown text]]` and resolve by title, compared the
//! way search compares (Turkish-safe folding), so `[[istanbul]]` finds
//! "İstanbul". Tags are the `#words` in the body. Every save puts the note in
//! the core's search index (private to Notes) and announces an `activity`
//! event with the title only.

use eon_office_sdk::{Client, Error};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

pub const COLLECTION: &str = "notes";
const MAX_IMPORT_FILES: usize = 20_000;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

pub type Outcome = Result<Value, Value>;

fn fail(code: &str, message: impl Into<String>) -> Value {
    json!({ "code": code, "message": message.into() })
}

fn sdk<T>(result: Result<T, Error>) -> Result<T, Value> {
    result.map_err(|e| {
        let mut out = fail(e.code(), e.to_string());
        if let Error::Protocol { data, .. } = &e {
            out["data"] = data.clone();
        }
        out
    })
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0))
}

/// Search-style folding: case and the Turkish i's, common diacritics.
pub fn fold(text: &str) -> String {
    text.chars()
        .filter_map(|c| {
            Some(match c {
                'I' | 'İ' | 'ı' | 'i' => 'i',
                'Ş' | 'ş' => 's',
                'Ğ' | 'ğ' => 'g',
                'Ç' | 'ç' => 'c',
                'Ö' | 'ö' => 'o',
                'Ü' | 'ü' => 'u',
                '\u{0300}'..='\u{036f}' => return None,
                other => other.to_lowercase().next().unwrap_or(other),
            })
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The targets of `[[links]]` in a body, in order, without duplicates.
pub fn links(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("[[") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("]]") else { break };
        let inner = &after[..end];
        let target = inner.split('|').next().unwrap_or_default().trim();
        if !target.is_empty()
            && !target.contains('\n')
            && !out.iter().any(|t| fold(t) == fold(target))
        {
            out.push(target.to_owned());
        }
        rest = &after[end + 2..];
    }
    out
}

/// The `#tags` in a body: `#` at a word start, then letters, digits, `-`, `_`, `/`.
/// Headings (`# Title`) are not tags; neither is `#` inside a word or a code span.
pub fn tags(body: &str) -> Vec<String> {
    let mut out = BTreeSet::new();
    let mut in_fence = false;
    for line in body.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let chars: Vec<char> = line.chars().collect();
        let mut in_code = false;
        for (i, c) in chars.iter().enumerate() {
            if *c == '`' {
                in_code = !in_code;
            }
            if *c != '#' || in_code || (i > 0 && !chars[i - 1].is_whitespace()) {
                continue;
            }
            let tag: String = chars[i + 1..]
                .iter()
                .take_while(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '/'))
                .collect();
            if tag.chars().any(char::is_alphabetic) {
                out.insert(fold(&tag));
            }
        }
    }
    out.into_iter().collect()
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn new_id() -> String {
    format!(
        "n{:x}{:x}",
        now_ms(),
        COUNTER.fetch_add(1, Ordering::Relaxed) % 4096
    )
}

pub struct Notes {
    pub client: Client,
}

impl Notes {
    pub fn new(client: Client) -> Notes {
        Notes { client }
    }

    fn activity(&self, id: &str, title: &str, kind: &str) {
        // Recent items in the launcher: the title, never the body. Failing to
        // announce never fails the save.
        let _ = self.client.publish(
            "activity",
            &json!({ "id": id, "title": title, "kind": kind }),
        );
    }

    fn index(&self, id: &str, note: &Value) {
        let _ = self.client.index(
            id,
            note["title"].as_str().unwrap_or_default(),
            note["body"].as_str().unwrap_or_default(),
            false,
        );
    }

    /// Every note's metadata (no bodies), newest first.
    pub fn list(&self) -> Outcome {
        let mut out = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let mut params = json!({ "collection": COLLECTION, "limit": 1000 });
            if let Some(a) = &after {
                params["after"] = json!(a);
            }
            let page = sdk(self.client.call("storage.list", params))?;
            let items = page["items"].as_array().cloned().unwrap_or_default();
            for item in &items {
                let doc = &item["doc"];
                out.push(json!({
                    "id": item["id"], "title": doc["title"], "notebook": doc["notebook"],
                    "tags": doc["tags"], "updated": doc["updated"], "rev": item["rev"],
                }));
            }
            if items.len() < 1000 {
                break;
            }
            after = items
                .last()
                .and_then(|i| i["id"].as_str())
                .map(str::to_owned);
        }
        out.sort_by(|a, b| b["updated"].as_u64().cmp(&a["updated"].as_u64()));
        Ok(json!({ "notes": out }))
    }

    pub fn get(&self, id: &str) -> Outcome {
        let found = sdk(self.client.get(COLLECTION, id))?
            .ok_or_else(|| fail("not_found", "no such note"))?;
        let mut note = found["doc"].clone();
        note["id"] = json!(id);
        note["rev"] = found["rev"].clone();
        self.activity(id, note["title"].as_str().unwrap_or_default(), "opened");
        Ok(note)
    }

    fn write(
        &self,
        id: &str,
        title: &str,
        body: &str,
        notebook: &str,
        created: u64,
        rev: Option<u64>,
    ) -> Outcome {
        let title = title.trim();
        if title.is_empty() || title.chars().count() > 200 {
            return Err(fail("invalid_params", "a title is 1-200 characters"));
        }
        let note = json!({
            "title": title, "body": body, "notebook": notebook.trim(),
            "tags": tags(body), "created": created, "updated": now_ms(),
        });
        let written = sdk(self.client.put(COLLECTION, id, &note, rev))?;
        self.index(id, &note);
        Ok(json!({ "id": id, "rev": written["rev"] }))
    }

    pub fn create(&self, title: &str, body: &str, notebook: &str) -> Outcome {
        let id = new_id();
        let out = self.write(&id, title, body, notebook, now_ms(), Some(0))?;
        self.activity(&id, title.trim(), "created");
        let _ = self.client.publish("note.created", &json!({ "id": id }));
        Ok(out)
    }

    /// Save with the revision the editor loaded; a stale save is refused with
    /// `conflict` so two windows never silently overwrite each other.
    pub fn save(&self, id: &str, title: &str, body: &str, notebook: &str, rev: u64) -> Outcome {
        let existing = sdk(self.client.get(COLLECTION, id))?
            .ok_or_else(|| fail("not_found", "no such note"))?;
        let created = existing["doc"]["created"].as_u64().unwrap_or_else(now_ms);
        let out = self.write(id, title, body, notebook, created, Some(rev))?;
        self.activity(id, title.trim(), "edited");
        Ok(out)
    }

    pub fn delete(&self, id: &str, rev: u64) -> Outcome {
        sdk(self.client.call(
            "storage.delete",
            json!({ "collection": COLLECTION, "id": id, "rev": rev }),
        ))?;
        let _ = self.client.call("search.remove", json!({ "id": id }));
        Ok(json!({ "deleted": id }))
    }

    /// The daily note for `date` (`YYYY-MM-DD`), created on first use.
    pub fn daily(&self, date: &str) -> Outcome {
        let ok = date.len() == 10
            && date.bytes().enumerate().all(|(i, b)| {
                if i == 4 || i == 7 {
                    b == b'-'
                } else {
                    b.is_ascii_digit()
                }
            });
        if !ok {
            return Err(fail("invalid_params", "a date is YYYY-MM-DD"));
        }
        let id = format!("daily-{date}");
        if sdk(self.client.get(COLLECTION, &id))?.is_none() {
            let body = format!("# {date}\n\n");
            self.write(&id, date, &body, "Daily", now_ms(), Some(0))?;
            self.activity(&id, date, "created");
        }
        self.get(&id)
    }

    /// Title -> id, folded, for every note.
    fn titles(&self) -> Result<BTreeMap<String, String>, Value> {
        let list = self.list()?;
        Ok(list["notes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|n| Some((fold(n["title"].as_str()?), n["id"].as_str()?.to_owned())))
            .collect())
    }

    /// The note a `[[link]]` names, if any.
    pub fn resolve(&self, title: &str) -> Outcome {
        Ok(json!({ "id": self.titles()?.get(&fold(title)) }))
    }

    /// Notes that link to `id`'s title.
    pub fn backlinks(&self, id: &str) -> Outcome {
        let target = sdk(self.client.get(COLLECTION, id))?
            .ok_or_else(|| fail("not_found", "no such note"))?;
        let title = fold(target["doc"]["title"].as_str().unwrap_or_default());
        let mut out = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let mut params = json!({ "collection": COLLECTION, "limit": 1000 });
            if let Some(a) = &after {
                params["after"] = json!(a);
            }
            let page = sdk(self.client.call("storage.list", params))?;
            let items = page["items"].as_array().cloned().unwrap_or_default();
            for item in &items {
                let body = item["doc"]["body"].as_str().unwrap_or_default();
                if item["id"] != id && links(body).iter().any(|l| fold(l) == title) {
                    out.push(json!({ "id": item["id"], "title": item["doc"]["title"] }));
                }
            }
            if items.len() < 1000 {
                break;
            }
            after = items
                .last()
                .and_then(|i| i["id"].as_str())
                .map(str::to_owned);
        }
        Ok(json!({ "backlinks": out }))
    }

    pub fn search(&self, query: &str) -> Outcome {
        let hits = sdk(self.client.search(query))?;
        Ok(
            json!({ "hits": hits.into_iter().filter(|h| h["app"] == "org.eon.office.notes").collect::<Vec<_>>() }),
        )
    }

    /// Import a folder of Markdown files (an Obsidian-style vault): one note
    /// per `.md` file, titled by its file name, in a notebook named after its
    /// folder. Bodies are kept as written, so `[[links]]` keep working.
    pub fn import(&self, folder: &Path) -> Outcome {
        let mut files = Vec::new();
        collect(folder, folder, &mut files)?;
        if files.len() > MAX_IMPORT_FILES {
            return Err(fail(
                "limit",
                format!("at most {MAX_IMPORT_FILES} files at once"),
            ));
        }
        let mut imported = 0;
        let mut skipped = Vec::new();
        let mut titles = BTreeSet::new();
        let mut bodies = Vec::new();
        for (path, notebook) in &files {
            let title = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            match std::fs::read_to_string(path) {
                Ok(body) => {
                    self.create(&title, &body, notebook)?;
                    titles.insert(fold(&title));
                    bodies.push(body);
                    imported += 1;
                }
                Err(_) => skipped.push(path.display().to_string()),
            }
        }
        let (mut total, mut resolved) = (0u64, 0u64);
        for body in &bodies {
            for link in links(body) {
                total += 1;
                // A link may name a heading or a block inside a note: `[[Note#Part]]`.
                let note = link.split('#').next().unwrap_or_default();
                if titles.contains(&fold(note)) {
                    resolved += 1;
                }
            }
        }
        Ok(
            json!({ "imported": imported, "skipped": skipped, "links": total, "resolved": resolved }),
        )
    }

    /// Answer what other applications ask Notes for: `open`, `focus`,
    /// `note.create`. Returns what the window should show, if anything.
    pub fn answer(&self, id: &Value, method: &str, params: &Value) -> Option<Value> {
        let (answer, show) = match method {
            "open" => {
                let note = params["id"].as_str().unwrap_or_default();
                match self.get(note) {
                    Ok(_) => (Ok(json!({ "opened": note })), Some(json!({ "open": note }))),
                    Err(e) => (Err(e), None),
                }
            }
            "focus" => (
                Ok(json!({ "focused": true })),
                Some(json!({ "focus": true })),
            ),
            "note.create" => match self.create(
                params["title"].as_str().unwrap_or("Untitled"),
                params["body"].as_str().unwrap_or_default(),
                params["notebook"].as_str().unwrap_or_default(),
            ) {
                Ok(created) => {
                    let note = created["id"].clone();
                    (Ok(created), Some(json!({ "open": note })))
                }
                Err(e) => (Err(e), None),
            },
            _ => (Err(fail("unknown_method", "not provided")), None),
        };
        let reply = answer.map_err(|e| {
            eon_office_sdk::AppError::new(
                e["code"].as_str().unwrap_or("error"),
                e["message"].as_str().unwrap_or_default(),
            )
        });
        let _ = self.client.answer(id, reply);
        show
    }
}

fn collect(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(std::path::PathBuf, String)>,
) -> Result<(), Value> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| fail("unreadable", format!("{}: {e}", dir.display())))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue; // .obsidian, .git, .trash
        }
        if path.is_dir() {
            collect(root, &path, out)?;
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("md"))
            && entry.metadata().is_ok_and(|m| m.len() <= MAX_FILE_BYTES)
        {
            let notebook = path
                .parent()
                .and_then(|p| p.strip_prefix(root).ok())
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .filter(|p| !p.is_empty())
                .unwrap_or_else(|| "Imported".to_owned());
            out.push((path, notebook));
        }
        if out.len() > MAX_IMPORT_FILES {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_are_found_with_aliases_and_once_each() {
        let body = "See [[Shopping List]] and [[shopping list|the list]], then [[İstanbul trip]].\n[[]] [[broken";
        assert_eq!(links(body), vec!["Shopping List", "İstanbul trip"]);
    }

    #[test]
    fn titles_fold_the_turkish_way() {
        assert_eq!(fold("İSTANBUL  Gezisi"), fold("istanbul gezisi"));
        assert_eq!(fold("IŞIK"), "isik");
    }

    #[test]
    fn tags_skip_headings_words_and_code() {
        let body = "# Heading\nA #todo and #proje/eon here, not#this, `#code`, #2026 alone.\n```\n#fenced\n```\n#Önemli";
        assert_eq!(tags(body), vec!["onemli", "proje/eon", "todo"]);
    }
}
