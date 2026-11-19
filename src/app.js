// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// EON Notes' interface. No network of its own; everything through the host.
// Note text is the person's, but it can be imported from anywhere, so it is
// rendered by building DOM nodes, never by parsing it as markup.

"use strict";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const $ = (id) => document.getElementById(id);

let strings = {};
let notes = [];
let current = null; // { id, rev, title, body, notebook }
let dirty = false;
let saveTimer = 0;

// ---- strings, preferences -----------------------------------------------------

function t(key, vars = {}) {
  let text = strings[key] ?? key;
  for (const [name, value] of Object.entries(vars)) text = text.replaceAll(`{${name}}`, String(value));
  return text;
}

async function loadStrings(locale) {
  const language = locale && locale !== "system"
    ? (locale.startsWith("tr") ? "tr" : "en")
    : ((navigator.language || "en").toLowerCase().startsWith("tr") ? "tr" : "en");
  strings = await invoke("strings", { language });
  document.documentElement.lang = language;
  for (const n of document.querySelectorAll("[data-i18n]")) n.textContent = t(n.dataset.i18n);
  for (const n of document.querySelectorAll("[data-i18n-label]")) n.setAttribute("aria-label", t(n.dataset.i18nLabel));
  for (const n of document.querySelectorAll("[data-i18n-placeholder]")) n.setAttribute("placeholder", t(n.dataset.i18nPlaceholder));
}

async function applyPrefs() {
  const p = await invoke("prefs");
  const root = document.documentElement;
  if (p.theme === "light" || p.theme === "dark") root.dataset.theme = p.theme; else delete root.dataset.theme;
  if (p.high_contrast) root.dataset.contrast = "high"; else delete root.dataset.contrast;
  if (p.reduce_motion) root.dataset.motion = "reduce"; else delete root.dataset.motion;
  root.style.setProperty("--scale", String(p.font_scale || 1));
  await loadStrings(p.locale);
}

function announce(text) {
  $("announcer").textContent = "";
  window.setTimeout(() => { $("announcer").textContent = text; }, 30);
}

function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") node.className = v;
    else if (k === "text") node.textContent = v;
    else if (k.startsWith("on")) node.addEventListener(k.slice(2), v);
    else node.setAttribute(k, v);
  }
  for (const c of children) if (c !== null && c !== undefined) node.append(c);
  return node;
}

const reason = (e) => (e && (e.message || e.code)) || String(e);

// ---- Markdown, to DOM ----------------------------------------------------------
// A deliberate subset: headings, paragraphs, emphasis, code, fenced code,
// quotes, lists, task boxes, links (shown, not followed), [[wikilinks]].

function inline(text) {
  const out = [];
  const pattern = /(\[\[([^\]|\n]+)(?:\|([^\]\n]+))?\]\])|(`[^`\n]+`)|(\*\*[^*\n]+\*\*)|(\*[^*\n]+\*|_[^_\n]+_)|(\[([^\]\n]+)\]\(([^)\s]+)\))/g;
  let last = 0;
  for (const m of text.matchAll(pattern)) {
    if (m.index > last) out.push(document.createTextNode(text.slice(last, m.index)));
    if (m[1]) {
      const target = m[2].trim();
      out.push(el("button", { class: "wikilink", type: "button", text: (m[3] || m[2]).trim(), onclick: () => followLink(target) }));
    } else if (m[4]) out.push(el("code", { text: m[4].slice(1, -1) }));
    else if (m[5]) out.push(el("strong", {}, ...inline(m[5].slice(2, -2))));
    else if (m[6]) out.push(el("em", {}, ...inline(m[6].slice(1, -1))));
    else if (m[7]) out.push(el("span", { class: "muted", title: m[9], text: m[8] }));
    last = m.index + m[0].length;
  }
  if (last < text.length) out.push(document.createTextNode(text.slice(last)));
  return out;
}

function render(markdown) {
  const nodes = [];
  const lines = markdown.replace(/\r\n/g, "\n").split("\n");
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (/^```/.test(line)) {
      const code = [];
      i++;
      while (i < lines.length && !/^```/.test(lines[i])) code.push(lines[i++]);
      i++;
      nodes.push(el("pre", {}, el("code", { text: code.join("\n") })));
      continue;
    }
    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    if (heading) { nodes.push(el(`h${heading[1].length}`, {}, ...inline(heading[2]))); i++; continue; }
    if (/^>\s?/.test(line)) {
      const quote = [];
      while (i < lines.length && /^>\s?/.test(lines[i])) quote.push(lines[i++].replace(/^>\s?/, ""));
      nodes.push(el("blockquote", {}, ...render(quote.join("\n"))));
      continue;
    }
    if (/^\s*([-*]|\d+\.)\s+/.test(line)) {
      const ordered = /^\s*\d+\./.test(line);
      const list = el(ordered ? "ol" : "ul");
      while (i < lines.length && /^\s*([-*]|\d+\.)\s+/.test(lines[i])) {
        const item = lines[i++].replace(/^\s*([-*]|\d+\.)\s+/, "");
        const task = /^\[([ xX])\]\s+(.*)$/.exec(item);
        if (task) {
          const box = el("input", { type: "checkbox", disabled: "", "aria-label": task[2] });
          box.checked = task[1] !== " ";
          list.append(el("li", {}, box, " ", ...inline(task[2])));
        } else list.append(el("li", {}, ...inline(item)));
      }
      nodes.push(list);
      continue;
    }
    if (line.trim() === "") { i++; continue; }
    const para = [];
    while (i < lines.length && lines[i].trim() !== "" && !/^(#{1,6}\s|```|>|\s*([-*]|\d+\.)\s)/.test(lines[i])) para.push(lines[i++]);
    nodes.push(el("p", {}, ...inline(para.join(" "))));
  }
  return nodes;
}

// ---- the list ------------------------------------------------------------------

function filtered() {
  const notebook = $("notebook-filter").value;
  return notes.filter((n) => !notebook || n.notebook === notebook);
}

function renderList(items = filtered()) {
  $("note-count").textContent = t("count", { count: items.length });
  $("empty").hidden = notes.length > 0 || current !== null;
  $("note-list").replaceChildren(...items.map((n) => el("li", {
    role: "option", id: `opt-${n.id}`, "aria-selected": String(current?.id === n.id),
    onclick: () => openNote(n.id),
  }, el("div", { text: n.title }), el("div", { class: "meta", text: [n.notebook, new Date(n.updated).toLocaleDateString(document.documentElement.lang)].filter(Boolean).join(" · ") }))));
  const active = current && $(`opt-${current.id}`);
  if (active) $("note-list").setAttribute("aria-activedescendant", active.id); else $("note-list").removeAttribute("aria-activedescendant");
}

async function refreshList() {
  notes = (await invoke("list")).notes;
  const books = [...new Set(notes.map((n) => n.notebook).filter(Boolean))].sort();
  const chosen = $("notebook-filter").value;
  $("notebook-filter").replaceChildren(el("option", { value: "", text: t("notebook.all") }), ...books.map((b) => el("option", { value: b, text: b })));
  $("notebook-filter").value = books.includes(chosen) ? chosen : "";
  $("notebooks").replaceChildren(...books.map((b) => el("option", { value: b })));
  renderList();
}

$("note-list").addEventListener("keydown", (event) => {
  const items = filtered();
  if (!items.length) return;
  const at = Math.max(0, items.findIndex((n) => n.id === current?.id));
  const moves = { ArrowDown: at + 1, ArrowUp: at - 1, Home: 0, End: items.length - 1 };
  if (event.key in moves) {
    event.preventDefault();
    openNote(items[Math.min(items.length - 1, Math.max(0, moves[event.key]))].id, false);
  } else if (event.key === "Enter") {
    event.preventDefault();
    $("note-body").focus();
  }
});
$("notebook-filter").addEventListener("change", () => renderList());

// ---- the editor ------------------------------------------------------------------

function setMode(preview) {
  $("mode-edit").setAttribute("aria-selected", String(!preview));
  $("mode-preview").setAttribute("aria-selected", String(preview));
  $("mode-edit").tabIndex = preview ? -1 : 0;
  $("mode-preview").tabIndex = preview ? 0 : -1;
  $("note-body").hidden = preview;
  $("note-preview").hidden = !preview;
  if (preview) $("note-preview").replaceChildren(...render($("note-body").value));
}
$("mode-edit").addEventListener("click", () => { setMode(false); $("note-body").focus(); });
$("mode-preview").addEventListener("click", () => setMode(true));
for (const tab of [$("mode-edit"), $("mode-preview")]) {
  tab.addEventListener("keydown", (e) => {
    if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
      e.preventDefault();
      const preview = tab.id === "mode-edit";
      setMode(preview);
      (preview ? $("mode-preview") : $("mode-edit")).focus();
    }
  });
}

async function openNote(id, focusBody = true) {
  if (dirty) await save();
  try {
    const note = await invoke("get", { id });
    current = { id, rev: note.rev, title: note.title, body: note.body, notebook: note.notebook || "", tags: note.tags || [] };
    $("editor").hidden = false;
    $("empty").hidden = true;
    document.querySelector("aside").hidden = false;
    $("note-title").value = note.title;
    $("note-notebook").value = note.notebook || "";
    $("note-body").value = note.body;
    $("save-status").textContent = "";
    $("save-status").classList.remove("error");
    $("delete-confirm").hidden = true;
    dirty = false;
    if (!$("note-preview").hidden) setMode(true);
    renderList();
    renderSide(note);
    if (focusBody) $("note-body").focus();
  } catch (e) {
    announce(t("error", { reason: reason(e) }));
  }
}

async function renderSide(note) {
  $("tags").replaceChildren(...(note.tags || []).map((tag) => el("span", { class: "tag", text: `#${tag}` })));
  try {
    const { backlinks } = await invoke("backlinks", { id: current.id });
    $("backlinks").replaceChildren(...(backlinks.length
      ? backlinks.map((b) => el("li", {}, el("button", { class: "wikilink", type: "button", text: b.title, onclick: () => openNote(b.id) })))
      : [el("li", { class: "muted", text: t("backlinks.none") })]));
  } catch { /* the panel is a help, not a requirement */ }
}

function markDirty() {
  dirty = true;
  $("save-status").textContent = t("unsaved");
  window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(save, 1500);
}
for (const id of ["note-title", "note-notebook", "note-body"]) $(id).addEventListener("input", markDirty);

async function save() {
  window.clearTimeout(saveTimer);
  if (!current || !dirty) return;
  $("save-status").textContent = t("saving");
  try {
    const out = await invoke("save", {
      id: current.id, title: $("note-title").value.trim() || t("title.placeholder"),
      body: $("note-body").value, notebook: $("note-notebook").value, rev: current.rev,
    });
    current.rev = out.rev;
    dirty = false;
    $("save-status").textContent = t("saved");
    await refreshList();
  } catch (e) {
    $("save-status").classList.add("error");
    if (e && e.code === "conflict") {
      dirty = false; // keep the text on screen; do not retry blindly
      $("save-status").textContent = t("conflict");
      announce(t("conflict"));
    } else {
      $("save-status").textContent = t("error", { reason: reason(e) });
    }
  }
}
$("save").addEventListener("click", save);
document.addEventListener("keydown", (e) => {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") { e.preventDefault(); save(); }
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "n") { e.preventDefault(); newNote(); }
});

async function newNote(title = t("title.placeholder"), body = "") {
  if (dirty) await save();
  try {
    const { id } = await invoke("create", { title, body, notebook: $("notebook-filter").value });
    await refreshList();
    await openNote(id);
    $("note-title").select();
    $("note-title").focus();
  } catch (e) {
    announce(t("error", { reason: reason(e) }));
  }
}
$("new-note").addEventListener("click", () => newNote());

$("today").addEventListener("click", async () => {
  const d = new Date();
  const date = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  try {
    const note = await invoke("daily", { date });
    await refreshList();
    await openNote(note.id);
  } catch (e) {
    announce(t("error", { reason: reason(e) }));
  }
});

$("delete").addEventListener("click", () => {
  $("delete-question").textContent = t("delete.confirm", { title: current.title });
  $("delete-confirm").hidden = false;
  $("delete-no").focus();
});
$("delete-no").addEventListener("click", () => { $("delete-confirm").hidden = true; $("delete").focus(); });
$("delete-yes").addEventListener("click", async () => {
  try {
    await invoke("delete", { id: current.id, rev: current.rev });
    current = null;
    dirty = false;
    $("editor").hidden = true;
    // The side panel describes the open note; with none open it says nothing.
    document.querySelector("aside").hidden = true;
    $("backlinks").replaceChildren();
    $("tags").replaceChildren();
    await refreshList();
    $("note-list").focus();
  } catch (e) {
    announce(t("error", { reason: reason(e) }));
  }
});

async function followLink(title) {
  const { id } = await invoke("resolve", { title });
  if (id) { openNote(id); return; }
  // No such note: offer to create it, as the link asks for.
  const box = $("save-status");
  box.replaceChildren(document.createTextNode(`${t("link.missing", { title })} `),
    el("button", { class: "secondary", type: "button", text: t("link.create", { title }), onclick: () => newNote(title, "") }));
}

// ---- search and import -------------------------------------------------------------

let searchTimer = 0;
$("search").addEventListener("input", () => {
  window.clearTimeout(searchTimer);
  searchTimer = window.setTimeout(async () => {
    const q = $("search").value.trim();
    if (!q) { renderList(); return; }
    try {
      const { hits } = await invoke("search", { query: q });
      const ids = new Set(hits.map((h) => h.id));
      renderList(notes.filter((n) => ids.has(n.id)));
    } catch (e) {
      announce(t("error", { reason: reason(e) }));
    }
  }, 200);
});

$("import-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  try {
    const r = await invoke("import", { folder: $("import-path").value });
    announce(t("import.done", { count: r.imported, resolved: r.resolved, links: r.links }));
    $("note-count").textContent = t("import.done", { count: r.imported, resolved: r.resolved, links: r.links });
    await refreshList();
  } catch (e) {
    announce(t("error", { reason: reason(e) }));
  }
});

// ---- links from other applications, and preference changes ---------------------------

async function show(payload) {
  if (!payload) return;
  await refreshList();
  if (payload.open) await openNote(payload.open);
}
listen("show", ({ payload }) => show(payload));
listen("prefs-changed", async () => {
  await applyPrefs();
  // Text the page builds itself is re-built in the new language.
  await refreshList();
  if (current) renderSide({ tags: current.tags });
});
window.addEventListener("blur", () => { if (dirty) save(); });

// ---- start -----------------------------------------------------------------------------

async function start() {
  await loadStrings("system");
  try {
    await invoke("status");
    await applyPrefs();
    $("banner").hidden = true;
    $("layout").hidden = false;
    await refreshList();
    const first = await invoke("initial");
    if (first && first.open) await openNote(first.open, false);
    else if (notes.length && !current) await openNote(notes[0].id, false);
    // Only take focus if the person has not already put it somewhere (they
    // may have pressed Ctrl+N while the list was loading).
    if (document.activeElement === document.body || document.activeElement === null) $("note-list").focus();
    document.body.dataset.ready = "1";
  } catch (e) {
    $("layout").hidden = true;
    $("banner").hidden = false;
    $("banner-text").textContent = t("open.failed", { reason: reason(e) });
  }
}
$("retry").addEventListener("click", start);
start();
