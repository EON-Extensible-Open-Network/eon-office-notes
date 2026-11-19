// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// Additional permission under GNU GPL version 3 section 7:
// see LICENSE-EXCEPTION.md (EON Module ABI Exception 1.0).

//! Notes, kept with the EON Office library.
//!
//! A note is a document in this application's `notes` collection:
//! `{ title, body (Markdown), notebook, tags, created, updated }`. Links are
//! `[[Title]]` or `[[Title|shown text]]` and resolve by title, compared the
//! way search compares (Turkish-safe folding), so `[[istanbul]]` finds
//! "İstanbul". Tags are the `#words` in the body. Every save puts the note in
//! the shared search index (private to Notes) and the person's recent list,
//! with the title only.

use eon_office_core::manifest::Manifest;
use eon_office_core::{links as eon_links, AppContext, Home};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

pub const COLLECTION: &str = "notes";
const MAX_IMPORT_FILES: usize = 20_000;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

pub type Outcome = Result<Value, Value>;

fn fail(code: &str, message: impl Into<String>) -> Value {
    json!({ "code": code, "message": message.into() })
}

/// A library error as the interface sees it: `{ code, message, data }`.
pub fn core<T>(result: eon_office_core::Result<T>) -> Result<T, Value> {
    result.map_err(|e| {
        let error = e.to_json();
        json!({ "code": error["data"]["code"], "message": error["message"], "data": error["data"] })
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
    ctx: Mutex<AppContext>,
}

impl Notes {
    /// Open Notes' store in `home`, registering Notes with `folder` as where
    /// its program lives (`None`: leave the registration's folder as it is).
    pub fn open(home: &Home, folder: Option<&Path>) -> Result<Notes, Value> {
        let manifest: Value = serde_json::from_str(crate::MANIFEST)
            .map_err(|e| fail("internal", format!("manifest.json: {e}")))?;
        let manifest = core(Manifest::from_json(manifest))?;
        Ok(Notes {
            ctx: Mutex::new(core(home.app(&manifest, folder))?),
        })
    }

    fn ctx(&self) -> MutexGuard<'_, AppContext> {
        self.ctx.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The person's preferences: language, theme, text size, motion, contrast.
    pub fn prefs(&self) -> Outcome {
        core(self.ctx().prefs())
    }

    /// Changes whenever a preference changes; the window polls it.
    pub fn prefs_version(&self) -> i64 {
        self.ctx().prefs_version().unwrap_or(0)
    }

    fn activity(&self, id: &str, title: &str, kind: &str) {
        // Recent items: the title, never the body. Failing to record never
        // fails the save.
        let _ = self.ctx().activity(id, title, kind);
    }

    fn index(&self, id: &str, note: &Value) {
        let _ = self.ctx().index(
            id,
            note["title"].as_str().unwrap_or_default(),
            note["body"].as_str().unwrap_or_default(),
            false,
        );
    }

    fn find(&self, id: &str) -> Result<Option<Value>, Value> {
        let found = core(self.ctx().get(COLLECTION, id))?;
        Ok((!found.is_null()).then_some(found))
    }

    fn page(&self, after: Option<&str>) -> Result<Vec<Value>, Value> {
        let page = core(self.ctx().list(COLLECTION, after, 1000))?;
        Ok(page["items"].as_array().cloned().unwrap_or_default())
    }

    /// Every note's metadata (no bodies), newest first.
    pub fn list(&self) -> Outcome {
        let mut out = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let items = self.page(after.as_deref())?;
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
        let found = self
            .find(id)?
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
        let written = core(self.ctx().put(COLLECTION, id, &note, rev))?;
        self.index(id, &note);
        Ok(json!({ "id": id, "rev": written["rev"] }))
    }

    pub fn create(&self, title: &str, body: &str, notebook: &str) -> Outcome {
        let id = new_id();
        let out = self.write(&id, title, body, notebook, now_ms(), Some(0))?;
        self.activity(&id, title.trim(), "created");
        Ok(out)
    }

    /// Save with the revision the editor loaded; a stale save is refused with
    /// `conflict` so two windows never silently overwrite each other.
    pub fn save(&self, id: &str, title: &str, body: &str, notebook: &str, rev: u64) -> Outcome {
        let existing = self
            .find(id)?
            .ok_or_else(|| fail("not_found", "no such note"))?;
        let created = existing["doc"]["created"].as_u64().unwrap_or_else(now_ms);
        let out = self.write(id, title, body, notebook, created, Some(rev))?;
        self.activity(id, title.trim(), "edited");
        Ok(out)
    }

    pub fn delete(&self, id: &str, rev: u64) -> Outcome {
        core(self.ctx().delete(COLLECTION, id, Some(rev)))?;
        let _ = self.ctx().unindex(id);
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
        if self.find(&id)?.is_none() {
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
        let target = self
            .find(id)?
            .ok_or_else(|| fail("not_found", "no such note"))?;
        let title = fold(target["doc"]["title"].as_str().unwrap_or_default());
        let mut out = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let items = self.page(after.as_deref())?;
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
        let hits = core(self.ctx().search(query, 50))?;
        Ok(json!({ "hits": hits["hits"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|h| h["app"] == crate::APP_ID)
            .collect::<Vec<_>>() }))
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

    /// What the window should do for an `eon-office://` link it was started
    /// with or handed: open a note by id, or come forward. Anything else —
    /// another application's link, an unknown note, a non-link — is nothing.
    pub fn link(&self, text: &str) -> Option<Value> {
        let link = eon_links::parse(text)?;
        if link.app != crate::APP_ID {
            return None;
        }
        if link.item.is_empty() {
            return Some(json!({ "focus": true }));
        }
        self.find(&link.item)
            .ok()
            .flatten()
            .map(|_| json!({ "open": link.item }))
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
