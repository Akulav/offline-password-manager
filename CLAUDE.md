# CLAUDE.md

Notes for working on **SeePass** (repo `offline-password-manager`, crate `seepass`), an offline password manager made
by Akulav. It never touches the network for vault data. Rust + eframe. Version 8.x is the Rust rewrite; `legacy/` holds
the old C# WinForms app (7.x), kept only for reference and import. See `docs/architecture.md`, `docs/security.md`,
`docs/migrating-from-7.md` and `docs/releasing.md`.

## Commands (same as CI)

```bash
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo check --locked --no-default-features
cargo test --locked
cargo build --locked --release
cargo run --example demo_vault               # sample vault for screenshots / manual checks
```

## Layout

- `src/crypto.rs` — key derivation (Argon2) and encryption (ChaCha20-Poly1305), `zeroize` for secrets
- `src/vault.rs` — vault format and storage; `src/legacy.rs` — import of 7.x data (SQLite via `rusqlite`)
- `src/generator.rs`, `src/strength.rs` — password generator and strength meter
- `src/settings.rs`, `src/install.rs`, `src/error.rs`, `src/lib.rs`, `src/main.rs`
- `src/ui/` — eframe app: `lock.rs`, `vault_view.rs`, `dialogs.rs`, `clipboard.rs`, `install_prompt.rs`, `task.rs`,
  `widgets.rs`, `tests.rs`
- `installer/seepass.iss` — Inno Setup script. **Never change its AppId.**

## Security rules

- **The vault format is frozen**: existing vaults must keep opening. Format changes need a version field bump and a
  migration path, plus tests that open the old format.
- Never log, print, or put in an error message any password, master key, or decrypted field. Zeroize secrets.
- **No network code at all.** SeePass doesn't even check for updates: running a newer download is the update
  (`src/install.rs`). Don't add HTTP dependencies.
- Clipboard copies of secrets must keep their auto-clear.
- Tests use generated throwaway vaults only; never commit a real vault.
- `legacy/` is reference code with weak crypto by design (it's why 8.x exists). Don't fix it or build on it; only
  `src/legacy.rs` reads its data.

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships. It is local only (`graphify-out/` is git-ignored).

Rules:
- When graphify-out/graph.json exists, run `graphify query "<question>"` before your first grep, search or source-file read for a codebase question, and put this rule in every subagent prompt that explores code. Read raw files only for lines the graph or an earlier read already located. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost). If graphify-out/ is missing, rebuild it the same way.
