<!--
SPDX-License-Identifier: GPL-3.0-or-later
SPDX-FileCopyrightText: 2026 EON contributors
-->

# Changelog

## 0.2.0 — stands alone

- **No core service, no launcher needed.** Notes links the `eon-office-core` library and
  opens its own store in the EON Office home folder; nothing else has to be installed or
  running (plan madde 43; `eon-docs/office/design/2026-11-19-library-core.md`).
- **One window:** a second start, or an `eon-office://org.eon.office.notes/<id>` link,
  goes to the running window. Replaces the `open`, `focus` and `note.create` methods.
- Shared preferences are noticed within two seconds of a change elsewhere.
- Fixed: after deleting the open note, the side panel kept describing it (and in the
  previous language). With no note open it is now hidden.
- Tests no longer need a running core; the window check starts Notes on its own.

## 0.1.0 — first version

- Markdown notes with a preview built as page elements; notebooks; `#tags`; `[[links]]`
  with aliases and a backlinks panel; links to missing notes offer to create them.
- Today's note, one per date.
- Search through the core with Turkish folding.
- Import of a Markdown folder (Obsidian-style vault) with links kept: 1,000 notes in about
  3 s in tests, every link to an existing note resolving.
- Saves refused on an out-of-date version instead of overwriting.
- Keyboard shortcuts and list navigation; live preferences from EON Launcher.
- Provides `open`, `focus`, `note.create`; publishes `activity` and `note.created`.
- Tests: unit, against a real core, `--selftest`, and the window driven through DevTools.
