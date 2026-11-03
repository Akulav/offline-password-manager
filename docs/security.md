# Security design

This page explains what SeePass protects against, how, and where its limits are.
If you find a problem, please open a private security advisory on GitHub
(**Security → Report a vulnerability**) rather than a public issue.

## Threat model

**SeePass is designed to protect you when someone gets a copy of your vault file**:
a stolen laptop or USB stick, a leaked backup, a synced folder, a curious
person with access to your disk. With a strong master password, the file alone
reveals nothing useful: not the entries, how many there are, or which sites
they belong to.

**It also limits exposure while you use it.** Secrets are wiped from memory on
lock and exit, the clipboard is cleared after a copy, and the vault auto-locks
when you're idle.

**It cannot protect you from malware already running as your user** while the
vault is unlocked. A keylogger can capture the master password. A process
that can read SeePass's memory can read decrypted entries. No password manager
fully solves this, and SeePass does not claim to.

Out of scope:
- A compromised OS or hardware.
- A weak master password. SeePass enforces a minimum, but Argon2 only slows
  guessing down; it does not stop it.
- Shoulder-surfing and screen recording.
- Copies the GUI toolkit (egui) makes of text while you type. These are not
  zeroized. They live only in process memory and are gone when the process exits.

## Cryptography

| Purpose | Algorithm | Parameters |
|---|---|---|
| Password → key | Argon2id (v1.3) | 64 MiB, 3 passes, 1 lane (default); 256 MiB, 4 passes ("extra strong"); 16-byte random salt |
| Encryption + integrity | XChaCha20-Poly1305 | 256-bit key, 192-bit random nonce per save, 128-bit tag |
| Random numbers | OS CSPRNG (`getrandom`) | salts, nonces, entry ids, generated passwords |

All primitives come from the audited [RustCrypto](https://github.com/RustCrypto)
crates. SeePass does not implement any cryptography of its own.

### Vault file format (version 1)

```text
offset  size  field
0       8     magic "SEEPASS\0"
8       2     format version (u16 LE) = 1
10      4     Argon2id memory cost in KiB (u32 LE)
14      4     Argon2id passes (u32 LE)
18      4     Argon2id parallelism (u32 LE)
22      16    Argon2id salt
38      24    XChaCha20 nonce
62      n+16  ciphertext of the JSON payload || Poly1305 tag
```

The **whole 62-byte header is authenticated** as associated data. Changing the
version, KDF parameters, salt or nonce makes decryption fail, exactly as
changing the ciphertext does. Before running Argon2, the reader rejects
parameters above 2 GiB / 64 passes / 16 lanes, so a hostile file can't make
SeePass exhaust memory.

The decrypted payload is JSON:

```json
{ "entries": [ { "id": "…", "title": "…", "username": "…", "password": "…",
                 "url": "…", "notes": "…", "created": 1762128000, "updated": 1762128000 } ] }
```

Wrong password and corrupted file are deliberately indistinguishable:
both give "wrong master password, or the vault file is corrupted".

### Saving

Each save re-encrypts the whole payload with a fresh nonce, writes it to a
temporary sibling file, `fsync`s it, and renames it over the vault. A crash or
power loss during a save leaves either the old vault or the new one, never half
of each. Before replacing the vault, the previous version is copied to
`vault.seepass.bak`. That copy is encrypted the same way.

### Settings

`settings.json` (theme, auto-lock and clipboard timers, a custom vault path) is
**not** encrypted, so the lock screen can apply your theme before unlock. It
holds no secrets.

## Memory hygiene

- The derived key is held in a `Zeroizing<[u8; 32]>` and overwritten on drop.
- Entries derive `ZeroizeOnDrop`, so locking (which drops the whole unlocked
  vault) overwrites every title, username, password and note.
- Password input buffers, decrypted plaintext and generated passwords are
  `Zeroizing` too.
- Argon2 and file I/O run on a worker thread, so the UI never freezes while
  deriving a key.

## Clipboard

Copying a secret stores a SHA-256 of what was copied. When the timer fires
(20 s by default), SeePass clears the clipboard **only if it still holds that
value**, so something you copied afterwards is left alone. On Windows, the copy
is flagged to stay out of clipboard history (Win+V) and cloud clipboard sync.
The clipboard is also cleared on lock and on exit.

## Master password policy

New master passwords need at least 10 characters and a
[zxcvbn](https://github.com/dropbox/zxcvbn) score of 3 or higher ("strong").
This rejects keyboard walks, dictionary words with digits stuck on, and the
like, which character-class rules let through.

## What 7.x did and why it changed

See the "Security" and "Removed" sections of the [changelog](../CHANGELOG.md#800---2026-11-03).
In short, 7.x had no integrity protection. It used one fixed key/IV pair for
every field, its derivation was 1000 rounds of PBKDF2-SHA1, it stored the
"vector" through a lossy text encoding, and it needed administrator rights.
`src/legacy.rs` documents its exact scheme so old vaults can still be imported.
