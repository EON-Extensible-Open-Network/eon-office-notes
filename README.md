<!--
SPDX-License-Identifier: GPL-3.0-or-later
SPDX-FileCopyrightText: 2026 EON contributors
-->

# EON Notes

**Notes that link to each other.** Markdown with a live preview, notebooks, `#tags`,
`[[links]]` with a backlinks panel, a daily note, search that understands Turkish, and an
import that takes a folder of Markdown files — an Obsidian-style vault — with its links
intact. The first application of [EON Office](https://github.com/EON-Extensible-Open-Network/eon-docs/blob/main/office/roadmap.md)'s Wave 1.

> **0.2.0** — formats `v0`, early. **Stands alone:** download it, run it, write. Nothing else
> has to be installed or running. Windows is where the window is checked end to end; Linux
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
- **Search**, Turkish-folded (`ISTANBUL` finds "İstanbul").
- **Import a vault**: one note per `.md` file, titled by its file name, in a notebook named
  after its folder; hidden folders (`.obsidian`, `.git`) skipped. Bodies are kept as
  written, so links keep working — measured: a 1,000-note vault imports in about 3 s with
  every link that names an existing note resolving.
- **Two windows never overwrite each other:** a save on an out-of-date version is refused,
  your text stays on screen, and nothing is merged behind your back.
- **Keyboard throughout:** Ctrl+N new note, Ctrl+S save, arrows in the list, Enter to the
  text, Edit/Preview as tabs. Follows the language, theme, text size, contrast and motion
  preferences shared by EON Office applications, live.
- **One window.** Starting Notes again — or following a link to a note — brings the open
  window forward instead of opening a second one.

**With other EON Office applications** (all optional): `eon-office://org.eon.office.notes/<id>`
opens a note, from any application that links to it. Notes adds what you open and edit to
the shared *Recent* list — titles only, never the text.

**What it asks for:** keep its data on this device (`storage`), make its notes searchable
(`search.index`, private to Notes), search (`search.query`), add titles to your recent list
(`events.publish`). Nothing else.

## Install

Download `eon-notes` from a release and run it — see the release notes for checksums. Or,
with EON Launcher: *Get applications* → *Include previews* → EON Notes. Your notes live in
the EON Office folder of your user profile (`%LOCALAPPDATA%\EON\Office` on Windows), so a
newer version finds them.

## Develop

```
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
node tools/ui-check.mjs --notes target/debug/eon-notes.exe --shots shots
```

`tests/notes.rs` runs Notes on the EON Office library in a private home folder, including
the 1,000-note vault import and a shared file from a newer version. `tools/ui-check.mjs`
starts Notes on its own and drives the real window through DevTools: create, type, save,
preview, follow a link, backlinks, tags, keyboard navigation, search, Today, an
`eon-office://` link handed to the running window by a second start, delete, live
preferences, and an accessible name for every control.

Licence `GPL-3.0-or-later` with the [EON Module ABI Exception](LICENSE-EXCEPTION.md).
Contributing: [CONTRIBUTING.md](CONTRIBUTING.md). Security: [SECURITY.md](SECURITY.md).
