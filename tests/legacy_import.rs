//! Checks the 7.x importer against vectors produced by the original C# crypto
//! code running on .NET Framework (see tests/fixtures/README.md).
#![cfg(feature = "legacy-import")]

use std::fs;
use std::io::Write;
use std::path::Path;

use seepass::legacy::LegacySource;
use serde_json::Value;

fn cases() -> Vec<Value> {
    let raw = fs::read_to_string("tests/fixtures/legacy_v7_vectors.json").unwrap();
    let v: Value = serde_json::from_str(&raw).unwrap();
    v["cases"].as_array().unwrap().clone()
}

/// Recreate a 7.x data folder (`settings.config` + `data.db`) from a case.
fn write_legacy_dir(case: &Value, dir: &Path) {
    // Newtonsoft wrote settings with a BOM-less UTF-8 writer, but be tolerant.
    fs::write(dir.join("settings.config"), serde_json::to_vec(&case["settings"]).unwrap()).unwrap();
    let conn = rusqlite::Connection::open(dir.join("data.db")).unwrap();
    conn.execute("CREATE TABLE data(username VARCRHAR(250), pass VARCRHAR(250),domain VARCRHAR(250))", []).unwrap();
    for r in case["rows"].as_array().unwrap() {
        conn.execute(
            "INSERT INTO data(username, pass, domain) VALUES(?1, ?2, ?3)",
            [r["username"].as_str(), r["pass"].as_str(), r["domain"].as_str()],
        )
        .unwrap();
    }
}

fn creds(case: &Value) -> (&str, &str, u32) {
    (case["user"].as_str().unwrap(), case["pass"].as_str().unwrap(), case["pim"].as_u64().unwrap() as u32)
}

fn assert_decrypts(case: &Value, src: &LegacySource) {
    let (user, pass, pim) = creds(case);
    let entries = src.decrypt(user, pass, pim).expect("import should succeed");
    let rows = case["rows"].as_array().unwrap();
    assert_eq!(entries.len(), rows.len());
    for (e, r) in entries.iter().zip(rows) {
        assert_eq!(e.title, r["plain_domain"].as_str().unwrap());
        assert_eq!(e.username, r["plain_username"].as_str().unwrap());
        assert_eq!(e.password, r["plain_pass"].as_str().unwrap());
    }
}

#[test]
fn imports_every_dotnet_vector_from_folder() {
    for case in cases() {
        let dir = tempfile::tempdir().unwrap();
        write_legacy_dir(&case, dir.path());
        assert_decrypts(&case, &LegacySource::load(dir.path()).unwrap());
    }
}

#[test]
fn imports_from_exported_zip() {
    let case = &cases()[0];
    let dir = tempfile::tempdir().unwrap();
    write_legacy_dir(case, dir.path());
    let zip_path = dir.path().join("data.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
    for name in ["settings.config", "data.db"] {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(&fs::read(dir.path().join(name)).unwrap()).unwrap();
    }
    zip.finish().unwrap();
    assert_decrypts(case, &LegacySource::load(&zip_path).unwrap());
}

#[test]
fn rejects_wrong_credentials() {
    let case = &cases()[0];
    let dir = tempfile::tempdir().unwrap();
    write_legacy_dir(case, dir.path());
    let src = LegacySource::load(dir.path()).unwrap();
    let (user, pass, pim) = creds(case);
    assert!(src.decrypt(user, "wrong", pim).is_err());
    assert!(src.decrypt("wrong", pass, pim).is_err());
    assert!(src.decrypt(user, pass, pim + 1).is_err());
}
