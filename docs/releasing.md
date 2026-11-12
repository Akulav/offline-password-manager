# Versioning & releases

## Versioning

SeePass follows [Semantic Versioning](https://semver.org/):

- **MAJOR**: incompatible vault format or behaviour (8.0.0 = Rust rewrite, new file format).
- **MINOR**: new features that still read and write existing vaults.
- **PATCH**: bug fixes only.

The version lives in **one place**: `version` in `Cargo.toml`. The app shows it
in the window title and status bar via `env!("CARGO_PKG_VERSION")`. The Windows
executable's file properties are generated from it at build time (`build.rs`).

Pre-releases use a suffix, e.g. `8.1.0-beta.1`. The release workflow marks any
version containing `-` as a GitHub **pre-release**.

The vault *file format* has its own version number (in the header, currently
`1`). It changes only when the on-disk layout changes, and a newer SeePass must
keep reading older formats.

## Cutting a release

1. Bump the version in `Cargo.toml`, then run `cargo check` so `Cargo.lock` picks it up.
2. In `CHANGELOG.md`, rename `## [Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD`,
   add a fresh empty `## [Unreleased]` above it, and update the links at the bottom.
3. Commit and push to `main`.

That's it: no tag needed. On every push to `main` (and every PR),
[`.github/workflows/build.yml`](../.github/workflows/build.yml) (the same logic as the other Akulav apps):

1. **Checks** formatting, clippy `-D warnings` and a build without default features, and **tests and
   builds** on Windows x64, Linux x64 (Ubuntu 22.04, so glibc 2.35+) and macOS arm64.
2. **Decides** whether a GitHub release already exists for the `Cargo.toml` version (as `vX.Y.Z` or
   `X.Y.Z`). If it does, it stops there; ordinary commits don't produce releases.
3. **Packages exactly one file per system**, the same file for installing and updating:
   `SeePass-setup.exe` (Inno Setup, `installer/seepass.iss`), `SeePass-linux-x64` and `SeePass-macos-arm64`
   (the binaries; they install themselves into a folder the user picks), plus `SHA256SUMS.txt`.
4. **Publishes** a GitHub Release named "SeePass vX.Y.Z", creating the tag `vX.Y.Z` on that commit. Its
   notes are that version's changelog section plus a downloads table.

Pushing a tag by hand (`vX.Y.Z` or `X.Y.Z`) also works; it must match `Cargo.toml`.

SeePass never touches the network, so installed copies don't look for updates: running a newer setup or
binary updates the installed copy in its folder.

If nothing shows up under Releases, check **Settings → Actions → General →
Workflow permissions**: the workflow asks for `contents: write`, which an
organization or repository policy can block.

## Other automation

- **Dependabot** opens weekly PRs for Cargo dependencies (minor and patch
  updates grouped into one) and for GitHub Actions versions.

## Code signing (not yet)

Releases are unsigned, so SmartScreen and Gatekeeper warn on first run. Signing
needs a certificate stored as repository secrets. Add a signing step to the
build jobs in `build.yml` once one is available.