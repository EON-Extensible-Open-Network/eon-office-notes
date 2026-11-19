// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// Drives the real EON Notes window through the WebView2 DevTools protocol.
// The core is started in a private home, Notes is installed and launched by
// the core -- as a person's launcher would -- and the window is checked.
// No dependencies (Node 22+), Windows, debug build of Notes.
//
//   node tools/ui-check.mjs --notes <eon-notes.exe> --core <eon-office-core.exe> [--shots <dir>]

import { spawn } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, existsSync, rmSync } from "node:fs";
import { createConnection } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const arg = (name) => { const at = process.argv.indexOf(name); return at > 0 ? process.argv[at + 1] : undefined; };
const notesExe = arg("--notes") && resolve(arg("--notes"));
const coreExe = arg("--core") && resolve(arg("--core"));
const shots = arg("--shots");
if (!notesExe || !coreExe) { console.error("usage: node tools/ui-check.mjs --notes <exe> --core <exe> [--shots <dir>]"); process.exit(2); }
if (shots) mkdirSync(shots, { recursive: true });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const home = mkdtempSync(join(tmpdir(), "eon-notes-ui-"));
const port = 9300 + Math.floor(Math.random() * 500);
let failures = 0;
const check = (name, ok, detail = "") => {
  console.log(`  ${ok ? "ok  " : "FAIL"}  ${name}${ok || !detail ? "" : `: ${detail}`}`);
  if (!ok) failures++;
};

function owner() {
  const endpoint = JSON.parse(readFileSync(join(home, "runtime", "endpoint.json"), "utf8"));
  const [host, p] = endpoint.address.split(":");
  const socket = createConnection({ host, port: Number(p) });
  let buffer = ""; let next = 1; const waiting = new Map();
  socket.setEncoding("utf8");
  socket.on("data", (chunk) => {
    buffer += chunk; let at;
    while ((at = buffer.indexOf("\n")) >= 0) {
      const m = JSON.parse(buffer.slice(0, at)); buffer = buffer.slice(at + 1);
      if (!m.method && waiting.has(m.id)) { waiting.get(m.id)(m); waiting.delete(m.id); }
    }
  });
  const call = (method, params = {}) => new Promise((ok, no) => {
    const id = next++;
    waiting.set(id, (m) => (m.error ? no(m.error) : ok(m.result)));
    socket.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
  });
  const token = readFileSync(join(home, "runtime", "owner.token"), "utf8").trim();
  const ready = new Promise((r) => socket.on("connect", r)).then(() => call("session.hello", { token }));
  return { call, ready, close: () => socket.destroy() };
}

let cdp;
async function devtools() {
  for (let i = 0; i < 300; i++) {
    try {
      const pages = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
      const page = pages.find((p) => p.type === "page" && p.webSocketDebuggerUrl);
      if (page) {
        const ws = new WebSocket(page.webSocketDebuggerUrl);
        await new Promise((ok, no) => { ws.onopen = ok; ws.onerror = no; });
        let next = 1; const waiting = new Map();
        ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && waiting.has(m.id)) { waiting.get(m.id)(m); waiting.delete(m.id); } };
        return { send: (method, params = {}) => new Promise((ok) => { const id = next++; waiting.set(id, ok); ws.send(JSON.stringify({ id, method, params })); }), close: () => ws.close() };
      }
    } catch { /* not yet */ }
    await sleep(200);
  }
  throw new Error("the Notes window's DevTools endpoint never appeared");
}
async function js(expression) {
  const r = await cdp.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (r.result?.exceptionDetails) throw new Error(JSON.stringify(r.result.exceptionDetails));
  return r.result?.result?.value;
}
async function until(expression, ms = 8000) {
  const end = Date.now() + ms;
  while (Date.now() < end) { if (await js(expression).catch(() => false)) return true; await sleep(100); }
  return false;
}
async function key(keyName, code = keyName, modifiers = 0, text) {
  const vk = { ArrowDown: 40, ArrowUp: 38, Home: 36, End: 35, ArrowRight: 39, ArrowLeft: 37, Enter: 13 }[keyName] ?? keyName.toUpperCase().charCodeAt(0);
  // Shortcuts with modifiers are raw key presses, as CDP expects; plain keys may carry text.
  await cdp.send("Input.dispatchKeyEvent", { type: modifiers ? "rawKeyDown" : "keyDown", key: keyName, code, modifiers, windowsVirtualKeyCode: vk, text });
  await cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key: keyName, code, modifiers, windowsVirtualKeyCode: vk });
}
// Ctrl+ shortcuts are sent as page-level keyboard events. Synthetic OS-level
// presses through CDP reach a background-started webview only intermittently,
// which tests the delivery, not Notes; arrows still go through CDP below.
async function shortcut(letter) {
  await js(`document.activeElement.dispatchEvent(new KeyboardEvent('keydown', { key: '${letter}', ctrlKey: true, bubbles: true, cancelable: true }))`);
}
async function type(text) { await cdp.send("Input.insertText", { text }); }
async function shot(name) {
  if (!shots) return;
  const r = await cdp.send("Page.captureScreenshot", { format: "png" });
  writeFileSync(join(shots, `notes-${name}.png`), Buffer.from(r.result.data, "base64"));
}

const core = spawn(coreExe, ["serve", "--home", home], {
  env: { ...process.env, EON_WEBVIEW_ARGS: `--remote-debugging-port=${port}` },
  stdio: "ignore",
});
let o;
try {
  console.log("EON Notes interface check");
  for (let i = 0; i < 100 && !existsSync(join(home, "runtime", "endpoint.json")); i++) await sleep(100);
  o = owner();
  await o.ready;
  await o.call("prefs.set", { key: "locale", value: "en" });
  const manifest = JSON.parse(readFileSync(new URL("../manifest.json", import.meta.url), "utf8"));
  manifest.entry = { command: notesExe };
  await o.call("apps.install", { manifest, grant: manifest.capabilities });
  await o.call("apps.launch", { app: manifest.id });
  cdp = await devtools();
  // The core starts Notes in the background, and Windows does not let a
  // background process take the foreground. A webview without focus drops
  // synthetic key presses now and then; focus emulation makes the page
  // behave as focused without touching what the person has in front of them.
  await cdp.send("Emulation.setFocusEmulationEnabled", { enabled: true });

  check("the window connects to the core and shows an empty notebook", await until(`document.body.dataset.ready === '1' && !document.getElementById('layout').hidden && !document.getElementById('empty').hidden`, 20000));
  await shot("01-empty");
  check("the search box is one line high", await js(`document.getElementById('search').getBoundingClientRect().height < 60`));

  // Ctrl+N makes a note and puts the cursor in its title.
  await js(`document.getElementById('note-list').focus()`);
  await shortcut("n");
  check("Ctrl+N creates a note and focuses its title", await until(`document.activeElement.id === 'note-title' && document.querySelectorAll('#note-list li').length === 1`),
    await js(`JSON.stringify({ active: document.activeElement.id || document.activeElement.tagName, items: document.querySelectorAll('#note-list li').length, editorHidden: document.getElementById('editor').hidden, status: document.getElementById('save-status').textContent, announcer: document.getElementById('announcer').textContent, hasFocus: document.hasFocus() })`));
  await type("İstanbul Trip");
  await js(`document.getElementById('note-body').focus()`);
  await type("Flights and #travel plans.\n\n- [x] passport\n- [ ] tickets\n");
  await shortcut("s");
  check("Ctrl+S saves", await until(`document.getElementById('save-status').textContent === 'Saved'`));

  // A second note linking to the first, by a differently-cased title.
  await shortcut("n");
  await until(`document.activeElement.id === 'note-title'`);
  await type("Packing");
  await js(`document.getElementById('note-body').focus()`);
  await type("For [[istanbul trip|the trip]]: **passport**, `adapter`.");
  await shortcut("s");
  await until(`document.getElementById('save-status').textContent === 'Saved'`);

  // Preview renders Markdown as elements, and the wikilink is a button.
  await js(`document.getElementById('mode-preview').click()`);
  check("preview renders bold, code and the wikilink as a button", await until(`(() => { const p = document.getElementById('note-preview'); return p.querySelector('strong')?.textContent === 'passport' && p.querySelector('code')?.textContent === 'adapter' && p.querySelector('button.wikilink')?.textContent === 'the trip'; })()`));
  await shot("02-preview");
  await js(`document.querySelector('#note-preview button.wikilink').click()`);
  check("following the link opens the linked note", await until(`document.getElementById('note-title').value === 'İstanbul Trip'`));
  check("the linked note shows its backlink", await until(`document.getElementById('backlinks').textContent.includes('Packing')`));
  check("its tags are listed", await until(`document.getElementById('tags').textContent.includes('#travel')`));
  check("task boxes render, checked as written", await until(`[...document.querySelectorAll('#note-preview input[type=checkbox]')].map(b => b.checked).join() === 'true,false'`));
  await shot("03-backlinks");

  // Keyboard: the list moves with arrows and Enter goes to the text.
  await js(`document.getElementById('mode-edit').click(); document.getElementById('note-list').focus()`);
  await key("ArrowDown");
  check("ArrowDown in the list opens the next note", await until(`document.getElementById('note-title').value === 'İstanbul Trip' || document.getElementById('note-title').value === 'Packing'`));
  const before = await js(`document.getElementById('note-title').value`);
  await key("ArrowUp");
  check("ArrowUp moves back", await until(`document.getElementById('note-title').value !== ${JSON.stringify(before)}`));
  await key("Enter");
  check("Enter moves to the text", await until(`document.activeElement.id === 'note-body'`));

  // Search, typed without Turkish letters.
  await js(`(() => { const q = document.getElementById('search'); q.value = 'ISTANBUL'; q.dispatchEvent(new Event('input')); })()`);
  // The title of one note and the link text in the other both match.
  check("search narrows the list, Turkish-folded", await until(`document.querySelectorAll('#note-list li').length === 2 && document.getElementById('note-list').textContent.includes('İstanbul Trip') && document.getElementById('note-list').textContent.includes('Packing')`));
  await js(`(() => { const q = document.getElementById('search'); q.value = ''; q.dispatchEvent(new Event('input')); })()`);

  // Today's note.
  await js(`document.getElementById('today').click()`);
  check("Today opens the daily note", await until(`/^\\d{4}-\\d{2}-\\d{2}$/.test(document.getElementById('note-title').value) && document.getElementById('note-notebook').value === 'Daily'`));

  // Another application asks Notes to create and show a note, through the core.
  const created = await o.call("apps.call", { app: manifest.id, method: "note.create", params: { title: "From Tasks", body: "Linked from a task." } });
  check("note.create from another app opens the new note", await until(`document.getElementById('note-title').value === 'From Tasks'`) && !!created.id);

  // Delete asks first.
  await js(`document.getElementById('delete').click()`);
  check("Delete asks first", await until(`!document.getElementById('delete-confirm').hidden && document.activeElement.id === 'delete-no'`));
  await js(`document.getElementById('delete-yes').click()`);
  check("confirming deletes the note", await until(`!document.getElementById('note-list').textContent.includes('From Tasks')`));

  // Preferences from the launcher apply here too.
  await o.call("prefs.set", { key: "locale", value: "tr" });
  check("a language change reaches Notes", await until(`document.getElementById('new-note').textContent === 'Yeni not'`));
  check("text the page builds is re-built in the new language", await until(`document.getElementById('note-count').textContent.endsWith(' not') && document.getElementById('notebook-filter').options[0].textContent === 'Tüm defterler'`));
  await o.call("prefs.set", { key: "font_scale", value: 1.25 });
  check("text size reaches Notes", await until(`getComputedStyle(document.documentElement).getPropertyValue('--scale').trim() === '1.25'`));
  await shot("04-turkish");

  const unnamed = await js(`[...document.querySelectorAll('button, input, select, textarea, [role=listbox]')].filter(e => {
    if (e.closest('[hidden]')) return false;
    const label = e.getAttribute('aria-label') || e.textContent.trim() || (e.id && document.querySelector('label[for="' + e.id + '"]')?.textContent.trim());
    return !label;
  }).map(e => e.outerHTML.slice(0, 80))`);
  check("every visible control has an accessible name", unnamed.length === 0, JSON.stringify(unnamed));
} catch (error) {
  failures++;
  console.error(`  FAIL  ${error.stack || error}`);
} finally {
  try { await o?.call("core.shutdown"); } catch { /* gone */ }
  o?.close();
  cdp?.close();
  await sleep(1500);
  try { core.kill(); } catch { /* gone */ }
  try { rmSync(home, { recursive: true, force: true }); } catch { /* in use */ }
}
console.log(failures ? `\n${failures} check(s) failed.` : "\nall checks passed");
process.exit(failures ? 1 : 0);
