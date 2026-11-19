<!--
SPDX-License-Identifier: GPL-3.0-or-later
SPDX-FileCopyrightText: 2026 EON contributors
-->

# Changelog

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
