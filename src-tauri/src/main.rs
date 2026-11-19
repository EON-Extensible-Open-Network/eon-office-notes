// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// Additional permission under GNU GPL version 3 section 7:
// see LICENSE-EXCEPTION.md (EON Module ABI Exception 1.0).

//! EON Notes -- the window.
//!
//! ```text
//! eon-notes                    open the window
//! eon-notes --selftest [FILE]  strings and interface checks
//! eon-notes --version
//! ```

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eon_notes::checks;
use eon_notes::notes::{Notes, Outcome};
use eon_office_sdk::{Client, Incoming};
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
}

impl Shared {
    fn notes(&self) -> Result<Arc<Notes>, Value> {
        self.notes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or_else(|| json!({ "code": "not_connected", "message": "not connected" }))
    }
}

type State<'a> = tauri::State<'a, Shared>;

/// Connect to the core as EON Notes and start answering other applications.
fn connect(app: &tauri::AppHandle, shared: &Shared) -> Outcome {
    if let Ok(notes) = shared.notes() {
        return Ok(json!({ "granted": notes.client.granted() }));
    }
    let client = Client::connect_app(eon_notes::APP_ID).map_err(|e| {
        let code = if e.code() == "unauthenticated" {
            "not_installed"
        } else if e.code() == "connection" {
            "no_core"
        } else {
            e.code()
        };
        json!({ "code": code, "message": e.to_string() })
    })?;
    let granted = client.granted();
    let notes = Arc::new(Notes::new(client));
    *shared.notes.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::clone(&notes));
    let handle = app.clone();
    std::thread::spawn(move || {
        while let Some(message) = notes.client.next(None) {
            match message {
                Incoming::Invoke {
                    id, method, params, ..
                } => {
                    if let Some(show) = notes.answer(&id, &method, &params) {
                        if let Some(window) = handle.get_webview_window("main") {
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                        let _ = handle.emit("show", show);
                    }
                }
                Incoming::Event { topic, .. } if topic == "core/prefs.changed" => {
                    let _ = handle.emit("prefs-changed", ());
                }
                Incoming::Shutdown => {
                    // The core asks Notes to leave: the interface saves first.
                    let _ = handle.emit("save-and-quit", ());
                    std::thread::sleep(Duration::from_millis(1500));
                    handle.exit(0);
                    break;
                }
                _ => {}
            }
        }
    });
    Ok(json!({ "granted": granted }))
}

#[tauri::command(async)]
fn strings(language: String) -> Value {
    eon_notes::strings(&language)
}

#[tauri::command(async)]
fn status(app: tauri::AppHandle, state: State<'_>) -> Outcome {
    connect(&app, &state)
}

#[tauri::command(async)]
fn prefs(state: State<'_>) -> Outcome {
    state
        .notes()?
        .client
        .prefs()
        .map_err(|e| json!({ "code": e.code(), "message": e.to_string() }))
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

fn window() {
    let app = tauri::Builder::default()
        .manage(Shared::default())
        .setup(|app| {
            main_window(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            strings, status, prefs, list, get, create, save, delete, daily, backlinks, resolve,
            search, import,
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
        _ => window(),
    }
}
