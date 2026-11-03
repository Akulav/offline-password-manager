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
tag containing `-` as a GitHub **pre-release**.

The vault *file format* has its own version number (in the header, currently
`1`). It changes only when the on-disk layout changes, and a newer SeePass must
keep reading older formats.

## Cutting a release

1. Make sure `main` is green in CI.
2. Bump the version in `Cargo.toml`, then run `cargo check` so `Cargo.lock` picks it up.
3. In `CHANGELOG.md`, rename `## [Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD`,
   add a fresh empty `## [Unreleased]` above it, and update the links at the bottom.
4. Commit: `git commit -am "Release vX.Y.Z"`.
5. Tag and push:

   ```bash
   git tag -a vX.Y.Z -m "SeePass X.Y.Z"
   ```

   ```bash
   git push origin main vX.Y.Z
   ```

That's it. [`.github/workflows/release.yml`](../.github/workflows/release.yml) then:

1. **Verifies** the tag equals the `Cargo.toml` version and that `CHANGELOG.md`
   has a section for it. Otherwise it fails before building anything.
2. **Builds and tests** release binaries on Windows x64, Linux x64 and macOS arm64.
3. **Packages** them: Windows as a `.zip` and a bare `.exe`, Linux and macOS as
   `.tar.gz`, each with README, LICENSE and CHANGELOG.
4. **Publishes** a GitHub Release named "SeePass X.Y.Z". Its notes are that
   version's changelog section, and it attaches every package plus `SHA256SUMS.txt`.

To rebuild an existing tag (say, after fixing the workflow), use
**Actions → Release → Run workflow** and enter the tag.

## Other automation

- **CI** ([`ci.yml`](../.github/workflows/ci.yml)) runs on every push to `main`
  and every PR: rustfmt, clippy `-D warnings` and tests on all three OSes, plus a
  build without default features.
- **Dependabot** opens weekly PRs for Cargo dependencies (minor and patch
  updates grouped into one) and for GitHub Actions versions.

## Code signing (not yet)

Releases are unsigned, so SmartScreen and Gatekeeper warn on first run. Signing
needs a certificate stored as repository secrets. Add a signing step between
**Build** and **Package** in `release.yml` once one is available.
