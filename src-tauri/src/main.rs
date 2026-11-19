// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// Additional permission under GNU GPL version 3 section 7:
// see LICENSE-EXCEPTION.md (EON Module ABI Exception 1.0).

//! EON Notes -- the window.
//!
//! ```text
//! eon-notes                    open the window
//! eon-notes eon-office://...   open the window on a note (a running window takes it)
//! eon-notes --selftest [FILE]  strings and interface checks
//! eon-notes --version
//! ```
//!
//! Notes stands alone: it opens its own store in the EON Office home folder
//! through the `eon-office-core` library. Nothing else has to be installed
//! or running.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eon_notes::checks;
use eon_notes::notes::{Notes, Outcome};
use eon_office_core::Home;
use serde_json::{json, Value};
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tauri::{Emitter, Manager};

const INDEX: &str = include_str!("../../src/index.html");
const SCRIPT: &str = include_str!("../../src/app.js");

#[derive(Default)]
struct Shared {
    notes: Mutex<Option<Arc<Notes>>>,
    /// The link Notes was started with, until the window has shown it.
    pending: Mutex<Option<String>>,
}

impl Shared {
    fn notes(&self) -> Result<Arc<Notes>, Value> {
        self.notes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or_else(|| json!({ "code": "not_open", "message": "the notes are not open" }))
    }
}

type State<'a> = tauri::State<'a, Shared>;

/// Open the notes in the EON Office home folder and register Notes there.
fn open(app: &tauri::AppHandle, shared: &Shared) -> Outcome {
    if shared.notes().is_ok() {
        return Ok(json!({ "open": true }));
    }
    let root = Home::default_location()
        .ok_or_else(|| json!({ "code": "no_home", "message": "no per-user data folder" }))?;
    let home = eon_notes::notes::core(Home::open(&root))?;
    let folder = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf));
    let notes = Arc::new(Notes::open(&home, folder.as_deref())?);
    *shared.notes.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::clone(&notes));
    // Preferences are shared with every EON Office application; notice a
    // change made elsewhere (the launcher) within two seconds.
    let handle = app.clone();
    std::thread::spawn(move || {
        let mut seen = notes.prefs_version();
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let now = notes.prefs_version();
            if now != seen {
                seen = now;
                let _ = handle.emit("prefs-changed", ());
            }
        }
    });
    Ok(json!({ "open": true }))
}

/// Show what a link asks for, bringing the window forward.
fn deliver(app: &tauri::AppHandle, link: &str) {
    let state = app.state::<Shared>();
    let Ok(notes) = state.notes() else {
        // Not open yet: the window asks for it once it is.
        *state.pending.lock().unwrap_or_else(PoisonError::into_inner) = Some(link.to_owned());
        return;
    };
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    if let Some(show) = notes.link(link) {
        let _ = app.emit("show", show);
    }
}

fn link_argument(args: &[String]) -> Option<String> {
    args.iter()
        .skip(1)
        .find(|a| a.starts_with(eon_office_core::links::SCHEME))
        .cloned()
}

#[tauri::command(async)]
fn strings(language: String) -> Value {
    eon_notes::strings(&language)
}

#[tauri::command(async)]
fn status(app: tauri::AppHandle, state: State<'_>) -> Outcome {
    open(&app, &state)
}

/// The link Notes was started with, once: what the window shows first.
#[tauri::command(async)]
fn initial(state: State<'_>) -> Outcome {
    let notes = state.notes()?;
    let link = state
        .pending
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();
    Ok(link.and_then(|l| notes.link(&l)).unwrap_or(Value::Null))
}

#[tauri::command(async)]
fn prefs(state: State<'_>) -> Outcome {
    state.notes()?.prefs()
}

#[tauri::command(async)]
fn list(state: State<'_>) -> Outcome {
    state.notes()?.list()
}

#[tauri::command(async)]
fn get(state: State<'_>, id: String) -> Outcome {
    state.notes()?.get(&id)
}

#[tauri::command(async)]
fn create(state: State<'_>, title: String, body: String, notebook: String) -> Outcome {
    state.notes()?.create(&title, &body, &notebook)
}

#[tauri::command(async)]
fn save(
    state: State<'_>,
    id: String,
    title: String,
    body: String,
    notebook: String,
    rev: u64,
) -> Outcome {
    state.notes()?.save(&id, &title, &body, &notebook, rev)
}

#[tauri::command(async)]
fn delete(state: State<'_>, id: String, rev: u64) -> Outcome {
    state.notes()?.delete(&id, rev)
}

#[tauri::command(async)]
fn daily(state: State<'_>, date: String) -> Outcome {
    state.notes()?.daily(&date)
}

#[tauri::command(async)]
fn backlinks(state: State<'_>, id: String) -> Outcome {
    state.notes()?.backlinks(&id)
}

#[tauri::command(async)]
fn resolve(state: State<'_>, title: String) -> Outcome {
    state.notes()?.resolve(&title)
}

#[tauri::command(async)]
fn search(state: State<'_>, query: String) -> Outcome {
    state.notes()?.search(&query)
}

#[tauri::command(async)]
fn import(state: State<'_>, folder: String) -> Outcome {
    state
        .notes()?
        .import(std::path::Path::new(folder.trim().trim_matches('"')))
}

fn main_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    #[allow(unused_mut)]
    let mut builder =
        tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
            .title("EON Notes")
            .inner_size(1100.0, 760.0)
            .min_inner_size(640.0, 480.0);
    // Debug builds only: a DevTools port for tools/ui-check.mjs.
    #[cfg(debug_assertions)]
    if let Ok(extra) = std::env::var("EON_WEBVIEW_ARGS") {
        builder = builder.additional_browser_args(&format!(
            "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection {extra}"
        ));
    }
    builder.build().map(|_| ())
}

fn window(link: Option<String>) {
    let shared = Shared::default();
    *shared
        .pending
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = link;
    let app = tauri::Builder::default()
        // First, so a second start hands over its link and exits before
        // anything else happens.
        .plugin(tauri_plugin_single_instance::init(
            |app, args, _cwd| match link_argument(&args) {
                Some(link) => deliver(app, &link),
                None => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.unminimize();
                        let _ = window.set_focus();
                    }
                }
            },
        ))
        .manage(shared)
        .setup(|app| {
            main_window(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            strings, status, initial, prefs, list, get, create, save, delete, daily, backlinks,
            resolve, search, import,
        ])
        .build(tauri::generate_context!());
    match app {
        Ok(app) => app.run(|_, _| {}),
        Err(e) => {
            eprintln!("EON Notes could not start: {e}");
            std::process::exit(1);
        }
    }
}

fn selftest() -> String {
    let mut report = format!("EON Notes {} self-test\n", env!("CARGO_PKG_VERSION"));
    let mut failed = false;
    let mut section = |name: &str, problems: Vec<String>| {
        if problems.is_empty() {
            let _ = writeln!(report, "  ok    {name}");
        } else {
            failed = true;
            for problem in problems {
                let _ = writeln!(report, "  FAIL  {name}: {problem}");
            }
        }
    };
    section(
        "strings (en, tr)",
        checks::catalogue_problems(eon_notes::EN, eon_notes::TR),
    );
    section("interface", checks::ui_problems(INDEX, SCRIPT));
    let manifest: Value = serde_json::from_str(eon_notes::MANIFEST).unwrap_or(Value::Null);
    let mut problems = Vec::new();
    if manifest["id"] != eon_notes::APP_ID || manifest["version"] != env!("CARGO_PKG_VERSION") {
        problems.push("manifest.json's id or version does not match this build".to_owned());
    }
    section("manifest", problems);
    report.push_str(if failed { "FAILED\n" } else { "ok\n" });
    report
}

fn main() {
    let mut arguments = std::env::args().skip(1);
    match arguments.next().as_deref() {
        Some("--version" | "-V") => println!("EON Notes {}", env!("CARGO_PKG_VERSION")),
        // Debug builds only: change a shared preference, as the launcher would,
        // so tools/ui-check.mjs can check that a running window follows it.
        #[cfg(debug_assertions)]
        Some("--set-pref") => {
            let (key, value) = (
                arguments.next().unwrap_or_default(),
                arguments.next().unwrap_or_default(),
            );
            let value: Value = serde_json::from_str(&value).unwrap_or(Value::String(value));
            let done = Home::default_location()
                .ok_or_else(|| "no home folder".to_owned())
                .and_then(|root| Home::open(&root).map_err(|e| e.to_string()))
                .and_then(|home| home.shared().map_err(|e| e.to_string()))
                .and_then(|shared| shared.set_pref(&key, &value).map_err(|e| e.to_string()));
            if let Err(e) = done {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        Some("--selftest") => {
            let report = selftest();
            match arguments.next() {
                Some(path) => {
                    if let Err(e) = std::fs::write(&path, &report) {
                        eprintln!("could not write {path}: {e}");
                        std::process::exit(2);
                    }
                }
                None => print!("{report}"),
            }
            if !report.ends_with("ok\n") {
                std::process::exit(1);
            }
        }
        _ => window(link_argument(&std::env::args().collect::<Vec<_>>())),
    }
}
