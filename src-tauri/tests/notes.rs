// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// Additional permission under GNU GPL version 3 section 7:
// see LICENSE-EXCEPTION.md (EON Module ABI Exception 1.0).

//! EON Notes against a real core: the roadmap's acceptance criteria that a
//! machine can check. Needs `EON_OFFICE_CORE_BIN`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::format_push_string
)]

use eon_notes::notes::Notes;
use eon_office_sdk::{Client, Incoming};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Core {
    child: Child,
    home: PathBuf,
}

impl Drop for Core {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn start(name: &str) -> (Core, Client, Notes) {
    let binary = std::env::var_os("EON_OFFICE_CORE_BIN").expect("set EON_OFFICE_CORE_BIN");
    let home = std::env::temp_dir().join(format!("eon-notes-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let child = Command::new(binary)
        .args(["serve", "--home"])
        .arg(&home)
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while eon_office_sdk::endpoint(&home).is_err() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(25));
    }
    let owner = Client::connect_owner(&home).unwrap();
    let manifest: Value = serde_json::from_str(eon_notes::MANIFEST).unwrap();
    owner
        .call(
            "apps.install",
            json!({ "manifest": manifest, "grant": manifest["capabilities"] }),
        )
        .unwrap();
    let token =
        std::fs::read_to_string(home.join("apps").join(eon_notes::APP_ID).join("token")).unwrap();
    let client = Client::connect(
        &eon_office_sdk::endpoint(&home).unwrap(),
        Some(eon_notes::APP_ID),
        token.trim(),
    )
    .unwrap();
    (Core { child, home }, owner, Notes::new(client))
}

#[test]
#[ignore = "needs EON_OFFICE_CORE_BIN"]
fn notes_links_daily_search_and_conflicts() {
    let (_core, owner, notes) = start("basic");
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
    notes
        .save(
            list["id"].as_str().unwrap(),
            "Packing",
            "first",
            "Personal",
            rev,
        )
        .unwrap();
    let stale = notes
        .save(
            list["id"].as_str().unwrap(),
            "Packing",
            "second",
            "Personal",
            rev,
        )
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

    // Search through the core, Turkish letters folded.
    let hits = notes.search("ISTANBUL").unwrap();
    assert!(hits["hits"]
        .as_array()
        .unwrap()
        .iter()
        .any(|h| h["id"] == trip["id"]));

    // The launcher's recent items get titles from Notes' activity events.
    let all = notes.list().unwrap();
    assert_eq!(all["notes"].as_array().unwrap().len(), 3);

    // Another application (here the owner) asks Notes to create a note through the core.
    let answering = std::thread::spawn(move || {
        while let Some(message) = notes.client.next(Some(Duration::from_secs(10))) {
            if let Incoming::Invoke {
                id, method, params, ..
            } = message
            {
                return (notes.answer(&id, &method, &params), notes);
            }
        }
        panic!("no invoke");
    });
    let created = owner
        .call_app(
            eon_notes::APP_ID,
            "note.create",
            json!({ "title": "From Tasks", "body": "hi" }),
        )
        .unwrap();
    let (show, notes) = answering.join().unwrap();
    assert_eq!(show.unwrap()["open"], created["id"]);
    assert_eq!(
        notes.get(created["id"].as_str().unwrap()).unwrap()["title"],
        "From Tasks"
    );
}

#[test]
#[ignore = "needs EON_OFFICE_CORE_BIN"]
fn a_thousand_note_vault_imports_with_its_links() {
    let (core, _owner, notes) = start("vault");
    // A vault shaped like a real one: folders, aliases, heading links, a few
    // links to notes that do not exist (they must count as unresolved).
    let vault = core.home.join("vault");
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
