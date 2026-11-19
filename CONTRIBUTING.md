<!--
SPDX-License-Identifier: GPL-3.0-or-later
SPDX-FileCopyrightText: 2026 EON contributors
-->

# Contributing to EON Notes

The organisation-wide guide,
[`.github/CONTRIBUTING.md`](https://github.com/EON-Extensible-Open-Network/.github/blob/main/CONTRIBUTING.md),
applies: DCO sign-off (`git commit -s`), Conventional Commits (`feat(notes): ...`).
By contributing you license your work under `GPL-3.0-or-later` with the EON Module ABI
Exception (`LICENSE-EXCEPTION.md`).

- **Every string in both languages.** Add it to `src-tauri/i18n/en.json` and `tr.json`
  with the same `{placeholders}`; `--selftest` fails otherwise.
- **Every control has a name and works from the keyboard.** `checks.rs` and
  `tools/ui-check.mjs` check what a machine can; say in the pull request how you checked
  the rest.
- **Notes text is untrusted when imported.** It reaches the page only as text nodes and
  elements the renderer builds; never through `innerHTML`. CI enforces it.

