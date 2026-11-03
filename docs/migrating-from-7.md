# Migrating from SeePass 7

SeePass 8 uses a new, stronger file format, so it doesn't open 7.x data
directly. It imports it once and then leaves the old files alone.

1. Start SeePass 8 and **create a new vault** with a master password. You can
   reuse your old password if it meets the new strength rules.
2. If `C:\PasswordManager` still exists, a **"SeePass 7 data found"** banner
   appears. Click **Import now…**. Otherwise open **Settings → Import from
   SeePass 7…** and select the `data.zip` you exported from 7.x.
3. Enter your **7.x username, password and PIM**, the same three things you
   typed into the old login screen.
4. Each old entry becomes a new one: the domain becomes the title, plus the
   username and password, with the note "Imported from SeePass 7". Running the
   import twice doesn't create duplicates.

When you have checked that everything came across, you can delete
`C:\PasswordManager` (deleting it needs administrator rights, because 7.x set it
up that way) and uninstall the old version.

## Troubleshooting

**"wrong username, password or PIM"**: 7.x checked all three together, so one
wrong character in any of them fails. It also could not tell `é` from `è`, or
any other non-ASCII characters apart. Typing the same characters you used
originally works.

**"could not determine the encryption vector's code page"**: 7.x saved part of
its key material through Windows' legacy text encoding. SeePass tries the
common Western, Central European, Cyrillic, Greek, Turkish, Baltic, Hebrew,
Arabic, Vietnamese and Thai code pages. If yours was different, please open an
issue and include your Windows display language. Never include your vault.

## Technical details

The exact 7.x scheme is documented at the top of [`src/legacy.rs`](../src/legacy.rs).
It is tested against vectors produced by the original C# code (see
[`tests/fixtures/README.md`](../tests/fixtures/README.md)).
