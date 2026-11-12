# Architecture

SeePass is one Cargo package with a **library** (`src/lib.rs`, all logic, no UI)
and a **binary** (`src/main.rs` + `src/ui/`, the egui desktop app). Keeping the
logic in the library means every security-relevant code path is unit-tested
without a window.

```text
src/
├── lib.rs          crate root, re-exports Error/Result, VERSION
├── crypto.rs       vault file format: Argon2id + XChaCha20-Poly1305, header parsing
├── vault.rs        Vault (unlocked state), Entry, atomic save, .bak, backups, merge, destroy
├── generator.rs    CSPRNG password generator + entropy estimate
├── strength.rs     zxcvbn wrapper, master-password policy
├── settings.rs     non-secret Settings (JSON) + AppPaths (in the install folder)
├── install.rs      install folder, self-install (Linux/macOS), moving pre-8.2 vaults in
├── legacy.rs       SeePass 7.x importer (feature "legacy-import")
├── error.rs        Error enum (thiserror)
├── main.rs         window setup, CLI args
└── ui/
    ├── mod.rs      App: screen state machine (Setup → Locked ⇄ Unlocked), auto-lock, toasts
    ├── lock.rs     first-run setup and unlock screens
    ├── vault_view.rs  toolbar, entry list, details, editor, generator window
    ├── dialogs.rs  settings window + modal dialogs (delete, change password, backup, import)
    ├── widgets.rs  theme, secret field, strength meter, status bar
    ├── clipboard.rs   ClipboardGuard (timed, ownership-checked clearing)
    ├── task.rs     Task<T>: run slow work on a thread, poll it from the UI
    └── tests.rs    headless UI tests (egui_kittest), optional screenshot rendering
tests/
├── legacy_import.rs           importer vs. vectors from the original .NET code
└── fixtures/legacy_v7_vectors.json
examples/demo_vault.rs         creates a vault with fake data for UI testing
```

## Screen state machine

```text
          no vault file                 unlock ok
 start ──────────────────▶ Setup ──────────────────────┐
   │                         ▲  create ok              ▼
   │  vault file exists      │                      Unlocked
   └──────────────────▶ Locked ◀──── lock / auto-lock ─┘
                              ◀──── "delete vault" goes to Setup
```

`App` owns a `Screen` enum. Each screen's `show` function returns an optional
`Next` transition, and `App::update` applies it after drawing. Leaving
`Unlocked` drops the `Vault`, which zeroizes the key and all entries, so
"locking" has no separate cleanup path that could be forgotten.

## Slow work off the UI thread

Argon2id takes roughly 0.3 to 2 s by design. Unlock, create, change password,
backup export/import and legacy import each run in a `Task<T>`: a thread plus
an `mpsc` channel, which wakes egui when done. The UI shows a spinner, keeps
repainting, and disables the buttons that would start a second task.

## Editor guard and undo

`Unlocked::request(Nav)` is the only way to leave the current view (select
another entry, start a new one, cancel). If the editor's draft differs from the
copy taken when editing started, it opens a "Discard changes?" dialog instead.
Locking skips this on purpose: security beats convenience. Deleting uses
`Vault::remove`, which hands the entry back. It is kept for 10 s for
`Vault::restore`, then dropped, which zeroizes it.

## Debug-only helpers

`SEEPASS_DEV_PASSWORD` auto-submits the lock screen. The code is inside
`#[cfg(debug_assertions)]`, so release binaries don't contain it.

## Feature flags

| Feature | Default | What it adds |
|---|---|---|
| `legacy-import` | on | `seepass::legacy`, SQLite (bundled), AES-CBC/PBKDF2-SHA1, zip, code-page tables |

## Testing

- `cargo test` runs about 30 unit tests: crypto round-trips and tamper detection,
  KDF parameter limits, vault CRUD and persistence, the check that no plaintext
  reaches disk, change password, backup/merge semantics, generator guarantees,
  strength policy, settings.
- `src/ui/tests.rs` drives the **real app** headlessly through egui's
  accessibility tree with `egui_kittest`. It clicks buttons, types text and
  presses keys, then asserts on what is on screen. It covers setup validation,
  wrong password, unlock, favorites section, reuse warning, edit + discard
  guard, new entry persisted to disk, delete + undo, settings, lock, and the
  light theme. It needs no window or GPU, so it runs in CI. With
  `SEEPASS_UI_SNAPSHOTS=<dir>` each step is also rendered to PNG with wgpu.
  The README screenshots come from there.
- `tests/legacy_import.rs` rebuilds real 7.x data folders and `data.zip`
  exports from vectors made by the **original C# code running on .NET
  Framework**, then checks every field decrypts exactly, including diacritics,
  emoji and awkward IV bytes. See `tests/fixtures/README.md`.
- CI runs clippy with `-D warnings`, rustfmt, and the tests on Windows, Linux
  and macOS, and also checks the build without default features.
