// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// Additional permission under GNU GPL version 3 section 7:
// see LICENSE-EXCEPTION.md (EON Module ABI Exception 1.0).

//! EON Notes on the EON Office library, in a home folder of its own: the
//! roadmap's acceptance criteria that a machine can check. Nothing else has
//! to be installed or running.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::format_push_string
)]

use eon_notes::notes::Notes;
use eon_office_core::Home;
use serde_json::json;
use std::path::PathBuf;
use std::time::Instant;

struct TempHome(PathBuf);

impl Drop for TempHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn start(name: &str) -> (TempHome, Home, Notes) {
    let dir = std::env::temp_dir().join(format!("eon-notes-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let home = Home::open(&dir).unwrap();
    let notes = Notes::open(&home, None).unwrap();
    (TempHome(dir), home, notes)
}

#[test]
fn notes_links_daily_search_and_conflicts() {
    let (_dir, home, notes) = start("basic");
    let trip = notes
        .create("İstanbul Trip", "Flights and #travel plans.", "Personal")
        .unwrap();
    let list = notes
        .create(
            "Packing",
            "For [[istanbul trip|the trip]]: passport. #travel #todo",
            "Personal",
        )
        .unwrap();
    let trip_id = trip["id"].as_str().unwrap();

    // Backlinks resolve however the title was typed.
    let back = notes.backlinks(trip_id).unwrap();
    assert_eq!(back["backlinks"][0]["id"], list["id"]);
    assert_eq!(notes.resolve("ISTANBUL TRIP").unwrap()["id"], trip["id"]);
    let note = notes.get(list["id"].as_str().unwrap()).unwrap();
    assert_eq!(note["tags"], json!(["todo", "travel"]));

    // Two windows: the second save on a stale revision is refused, not merged silently.
    let rev = note["rev"].as_u64().unwrap();
    let id = list["id"].as_str().unwrap();
    notes.save(id, "Packing", "first", "Personal", rev).unwrap();
    let stale = notes
        .save(id, "Packing", "second", "Personal", rev)
        .unwrap_err();
    assert_eq!(stale["code"], "conflict");

    // Daily note: one per date, created once.
    let a = notes.daily("2026-11-19").unwrap();
    let b = notes.daily("2026-11-19").unwrap();
    assert_eq!(
        (a["id"].clone(), a["notebook"].clone()),
        (b["id"].clone(), json!("Daily"))
    );
    assert!(notes.daily("19.11.2026").is_err());

    // Search, Turkish letters folded.
    let hits = notes.search("ISTANBUL").unwrap();
    assert!(hits["hits"]
        .as_array()
        .unwrap()
        .iter()
        .any(|h| h["id"] == trip["id"]));
    assert_eq!(notes.list().unwrap()["notes"].as_array().unwrap().len(), 3);

    // The person's recent list gets titles, never bodies.
    let recent = home.shared().unwrap().recent(10).unwrap();
    let first = &recent["items"][0];
    assert_eq!(first["app"], eon_notes::APP_ID);
    assert!(recent.to_string().contains("Packing"));
    assert!(!recent.to_string().contains("passport"));

    // Deleting removes it from search too.
    let fresh = notes.get(trip_id).unwrap();
    notes
        .delete(trip_id, fresh["rev"].as_u64().unwrap())
        .unwrap();
    assert!(!notes.search("ISTANBUL").unwrap()["hits"]
        .as_array()
        .unwrap()
        .iter()
        .any(|h| h["id"] == trip["id"]));
}

#[test]
fn a_link_argument_opens_the_note() {
    let (_dir, _home, notes) = start("link");
    let note = notes.create("Linked", "body", "").unwrap();
    let id = note["id"].as_str().unwrap();
    assert_eq!(
        notes.link(&format!("eon-office://org.eon.office.notes/{id}")),
        Some(json!({ "open": id }))
    );
    // The app's own address with no item brings the window forward.
    assert_eq!(
        notes.link("eon-office://org.eon.office.notes"),
        Some(json!({ "focus": true }))
    );
    // Another application's link, an unknown note, or not a link: nothing to show.
    assert_eq!(notes.link("eon-office://org.eon.office.tasks/x"), None);
    assert_eq!(notes.link("eon-office://org.eon.office.notes/nope"), None);
    assert_eq!(notes.link("--selftest"), None);
}

#[test]
fn notes_work_with_no_shared_file_features() {
    // A shared file from a newer EON Office: notes still open, save and list.
    let dir = std::env::temp_dir().join(format!("eon-notes-newer-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let home = Home::open(&dir).unwrap();
    drop(home.shared().unwrap());
    rusqlite_set_version(&dir.join("shared.sqlite"), 99);
    let notes = Notes::open(&home, None).unwrap();
    let made = notes.create("Still here", "text", "").unwrap();
    assert_eq!(
        notes.get(made["id"].as_str().unwrap()).unwrap()["title"],
        "Still here"
    );
    assert_eq!(notes.list().unwrap()["notes"].as_array().unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

fn rusqlite_set_version(path: &std::path::Path, version: i64) {
    // The library's own file format: set the schema version by hand.
    let conn = eon_office_core::db::open(path, eon_office_core::db::Schema::Shared).unwrap();
    conn.pragma_update(None, "user_version", version).unwrap();
}

#[test]
fn a_thousand_note_vault_imports_with_its_links() {
    let (dir, _home, notes) = start("vault");
    // A vault shaped like a real one: folders, aliases, heading links, a few
    // links to notes that do not exist (they must count as unresolved).
    let vault = dir.0.join("vault");
    let mut rng = 0x2545_f491_4f6c_dd1du64;
    let mut next = |n: u64| {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng % n
    };
    let titles: Vec<String> = (0..1000).map(|i| format!("Not {i} – Özet")).collect();
    let mut planted_broken = 0;
    let mut planted_links = 0;
    for (i, title) in titles.iter().enumerate() {
        let folder = vault.join(["Work", "Personal", "Projects/EON", "Reading"][i % 4]);
        std::fs::create_dir_all(&folder).unwrap();
        let mut body = format!("# {title}\n\nSome text #tag{}.\n", i % 7);
        for _ in 0..3 {
            let target = &titles[next(1000) as usize];
            body.push_str(&match next(3) {
                0 => format!("See [[{target}]].\n"),
                1 => format!("See [[{target}|this]].\n"),
                _ => format!("See [[{target}#Section]].\n"),
            });
            planted_links += 1;
        }
        if i % 200 == 0 {
            body.push_str("And [[A note that was never written]].\n");
            planted_broken += 1;
            planted_links += 1;
        }
        std::fs::write(folder.join(format!("{title}.md")), body).unwrap();
    }
    std::fs::create_dir_all(vault.join(".obsidian")).unwrap();
    std::fs::write(
        vault.join(".obsidian").join("app.md"),
        "settings, not a note",
    )
    .unwrap();

    let started = Instant::now();
    let report = notes.import(&vault).unwrap();
    let took = started.elapsed();
    assert_eq!(report["imported"], 1000, "hidden folders are skipped");
    let (links, resolved) = (
        report["links"].as_u64().unwrap(),
        report["resolved"].as_u64().unwrap(),
    );
    // `links` counts distinct targets per note, so it can be below what was planted.
    assert!(links <= planted_links);
    let share = resolved as f64 / links as f64;
    assert!(share >= 0.99, "{resolved}/{links} resolved");
    assert_eq!(
        links - resolved,
        planted_broken,
        "exactly the planted broken links fail"
    );
    eprintln!("imported 1000 notes in {took:?}; {resolved}/{links} links resolve");

    let listed = notes.list().unwrap();
    assert_eq!(listed["notes"].as_array().unwrap().len(), 1000);
    let books: std::collections::BTreeSet<&str> = listed["notes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|n| n["notebook"].as_str())
        .collect();
    assert!(books.contains("Projects/EON"));
}
