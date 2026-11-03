# Changelog

All notable changes to SeePass are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and SeePass uses
[Semantic Versioning](https://semver.org/). The release workflow publishes the
section whose heading matches the pushed tag as that release's notes.

## [Unreleased]

## [8.1.0] - 2026-11-03

A UI overhaul. Vaults stay compatible both ways: 8.0 opens 8.1 vaults and ignores the new fields.

### Added
- **Favorites.** Star an entry (button or `S`) to pin it to a "Favorites" section at the top.
- **Reused-password warning** on the entry page and live in the editor.
- **Password age.** SeePass tracks when each password last changed and suggests
  rotating ones older than a year.
- **Undo delete.** A deleted entry can be restored for 10 seconds.
- **Unsaved-changes guard.** Leaving the editor with edits asks before discarding them.
- **Sort** the list A–Z or by most recently edited.
- **Keyboard navigation:** ↑/↓ move through the list, `E`/Enter edits, Ctrl+C copies
  the password, Ctrl+B copies the username, `Del` deletes.
- **Status bar countdowns** for auto-lock and clipboard clearing.
- A "N failed attempts" counter on the lock screen.
- Headless UI tests (`egui_kittest`) covering setup, unlock, details, editing,
  discard, generator, delete/undo, settings and lock. They can also render screenshots.
- Debug builds only: `SEEPASS_DEV_PASSWORD` auto-unlocks, for UI work.

### Changed
- New look: colored avatar initials, card-style entry pages with strength pills,
  rounded widgets, and a refined dark and light palette.
- Entry list rows show title and username with clear selection and hover states.
  They are also exposed to screen readers.
- The editor uses a single-column form with labels above the fields and a live avatar preview.
- The generator puts the password first and colors the entropy readout.
- The settings window is grouped into sections, scrolls on small screens, and has a Dark/Light toggle.
- The empty vault shows clear first steps (add, import backup, import from SeePass 7).
  A non-empty vault shows a shortcut cheat-sheet.
- `demo_vault` example now includes a favorite, a reused password and an old password.

## [8.0.0] - 2026-11-03

A complete rewrite in Rust. The 7.x C# code is kept under `legacy/` for reference.

### Security
- New single-file vault: **Argon2id** key derivation (64 MiB / 3 passes by
  default, 256 MiB optional) and **XChaCha20-Poly1305** authenticated encryption.
  The header (format version, KDF params, salt, nonce) is authenticated too, so
  tampering is detected rather than silently decrypting to garbage.
- A fresh random nonce on every save. 7.x reused one key and IV for every field
  it ever encrypted, so identical passwords produced identical ciphertext.
- Secrets are wiped from memory (`zeroize`) when the vault locks or the app exits.
- Copied passwords are cleared from the clipboard after 20 s (configurable). On
  Windows they are also kept out of clipboard history (Win+V) and cloud sync.
- Auto-lock after 5 minutes of inactivity (configurable). The vault locks in
  place; 7.x restarted the whole application.
- Master passwords are scored with zxcvbn, and weak ones are rejected.
  `Password1234` passed the 7.x rules; it is refused now.
- Hostile vault headers (huge Argon2 memory requests) are rejected before any
  work is done.
- No SQL, so there is no SQL built from string concatenation.

### Added
- Edit entries (7.x could only add and delete), plus website and notes fields.
- Instant search across title, username, website and notes.
- Password generator (OS CSPRNG, character classes, look-alike filter, entropy estimate).
- Password strength meter with crack-time estimate.
- Change master password.
- Encrypted backup export with its own password, and import that merges by entry id.
- One-click import from SeePass 7.x (data folder or exported `data.zip`), checked
  against vectors produced by the original .NET code.
- Automatic previous-version backup (`vault.seepass.bak`) and atomic saves.
- Linux and macOS builds.
- Keyboard shortcuts: Ctrl+N new, Ctrl+F search, Ctrl+S save, Ctrl+L lock.

### Changed
- Data lives in the per-user profile (`%APPDATA%\SeePass` on Windows). No
  administrator rights, no `C:\PasswordManager`, no installer needed.
- One portable executable, with no DLLs to keep beside it.
- Unlocking needs only the master password. The 7.x username and PIM are
  replaced by tunable Argon2id cost.

### Removed
- The "integrity check" that compared the executable's MD5 with a hash
  downloaded from the same GitHub repo. Releases now publish `SHA256SUMS.txt`.
- The "secure erase" overwrite loop. On SSDs and journaling filesystems it
  never guaranteed anything. With authenticated encryption, deleting the
  file is enough: without the password the data cannot be read.
- The SeePass installer.

## [7.1.1]

Last C# / WinForms release. See `legacy/`.

[Unreleased]: https://github.com/Akulav/OfflinePasswordManager/compare/v8.1.0...HEAD
[8.1.0]: https://github.com/Akulav/OfflinePasswordManager/compare/v8.0.0...v8.1.0
[8.0.0]: https://github.com/Akulav/OfflinePasswordManager/releases/tag/v8.0.0
