<!--
SPDX-License-Identifier: GPL-3.0-or-later
SPDX-FileCopyrightText: 2026 EON contributors
-->

# EON Notes

**Notes that link to each other.** Markdown with a live preview, notebooks, `#tags`,
`[[links]]` with a backlinks panel, a daily note, search that understands Turkish, and an
import that takes a folder of Markdown files — an Obsidian-style vault — with its links
intact. The first application of [EON Office](https://github.com/EON-Extensible-Open-Network/eon-docs/blob/main/office/roadmap.md)'s Wave 1.

> **0.1.0** — protocol `v0`, early. Windows is where the window is checked end to end; Linux
> and macOS build and pass the same tests without a window.

## What it does

- **Write in Markdown**, see it rendered: headings, emphasis, code, quotes, lists, task
  boxes. The preview is built as page elements, never as raw HTML, so an imported file
  cannot run anything.
- **Link** with `[[Title]]` or `[[Title|shown text]]`; follow a link with a click; the panel
  on the right lists every note that links here. A link to a note that does not exist offers
  to create it. Titles match the way search does: `[[istanbul trip]]` finds "İstanbul Trip".
- **Notebooks** (a field, with suggestions), **tags** (`#word` in the text), **Today** (one
  note per date, in the *Daily* notebook).
- **Search** through the EON Office core, Turkish-folded (`ISTANBUL` finds "İstanbul").
- **Import a vault**: one note per `.md` file, titled by its file name, in a notebook named
  after its folder; hidden folders (`.obsidian`, `.git`) skipped. Bodies are kept as
  written, so links keep working — measured: a 1,000-note vault imports in about 3 s with
  every link that names an existing note resolving.
- **Two windows never overwrite each other:** a save on an out-of-date version is refused,
  your text stays on screen, and nothing is merged behind your back.
- **Keyboard throughout:** Ctrl+N new note, Ctrl+S save, arrows in the list, Enter to the
  text, Edit/Preview as tabs. Follows the language, theme, text size, contrast and motion
  preferences set in EON Launcher, live.

**For other applications** (through the core, if you allow them `call:org.eon.office.notes`):
`open` a note by id, `focus` the window, `note.create` from a title and Markdown. Notes
announces what you do with a note (`activity`, title only) so EON Launcher can show it under
*Recent*.

**What it asks for:** keep its data on this device (`storage`), make its notes searchable to
itself (`search.index`, private), search (`search.query`), announce changes
(`events.publish`). Nothing else.

## Install

EON Launcher → *Get applications* → *Include previews* → EON Notes. Or from a release:
see its notes. EON Notes needs the EON Office core running (EON Launcher starts it).

## Develop

```
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
EON_OFFICE_CORE_BIN=/path/to/eon-office-core cargo test -- --include-ignored
node tools/ui-check.mjs --notes target/debug/eon-notes.exe --core /path/to/eon-office-core.exe --shots shots
```

`tests/notes.rs` runs Notes against a real core, including the 1,000-note vault import.
`tools/ui-check.mjs` has the core install and launch Notes, then drives the real window
through DevTools: create, type, save, preview, follow a link, backlinks, tags, keyboard
navigation, search, Today, a `note.create` from another application, delete, live
preferences, and an accessible name for every control.

Licence `GPL-3.0-or-later` with the [EON Module ABI Exception](LICENSE-EXCEPTION.md).
Contributing: [CONTRIBUTING.md](CONTRIBUTING.md). Security: [SECURITY.md](SECURITY.md).
