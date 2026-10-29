use mycrates::model::IndexEntry;
use mycrates::publish::Publisher;
use mycrates::store::{NativeRecord, Store};
use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use tempfile::tempdir;

fn sample_entry(name: &str, vers: &str, cksum: &str) -> IndexEntry {
    IndexEntry {
        name: name.to_string(),
        vers: vers.to_string(),
        deps: vec![],
        cksum: cksum.to_string(),
        features: BTreeMap::new(),
        yanked: false,
        links: None,
        rust_version: None,
        v: Some(1),
        extra: BTreeMap::new(),
    }
}

#[test]
fn generation_determinism_and_noop() {
    let dir1 = tempdir().unwrap();
    let store1 = Store::new(dir1.path().join("state"));
    let site1 = dir1.path().join("site");

    let dir2 = tempdir().unwrap();
    let store2 = Store::new(dir2.path().join("state"));
    let site2 = dir2.path().join("site");

    let data = b"sample-crate-tarball-bytes";
    let sha = store1.ingest_blob(Cursor::new(data)).unwrap();
    let sha2 = store2.ingest_blob(Cursor::new(data)).unwrap();
    assert_eq!(sha, sha2);

    let rec = NativeRecord {
        name: "foo".to_string(),
        vers: "1.0.0".to_string(),
        checksum: sha.clone(),
        yanked: false,
        entry: sample_entry("foo", "1.0.0", &sha),
    };
    store1.put_native_record(&rec).unwrap();
    store2.put_native_record(&rec).unwrap();

    let pub1 = Publisher::new(&store1, site1.clone(), "http://localhost:8080".to_string());
    let res1 = pub1.publish().unwrap();
    assert!(!res1.is_noop);

    let pub2 = Publisher::new(&store2, site2.clone(), "http://localhost:8080".to_string());
    let res2 = pub2.publish().unwrap();
    assert!(!res2.is_noop);

    // Identical generation ID across independent state and site dirs
    assert_eq!(res1.generation_id, res2.generation_id);

    // Second publish on site1 detects no-op
    let res1_again = pub1.publish().unwrap();
    assert!(res1_again.is_noop);
    assert_eq!(res1_again.generation_id, res1.generation_id);

    // Verify current symlink
    let current_target = fs::read_link(site1.join("current")).unwrap();
    assert_eq!(
        current_target.to_string_lossy(),
        format!("generations/{}", res1.generation_id)
    );
}

#[test]
fn missing_blob_rejected_before_switch() {
    let dir = tempdir().unwrap();
    let store = Store::new(dir.path().join("state"));
    let site = dir.path().join("site");

    store.ensure_dirs().unwrap();
    let rec = NativeRecord {
        name: "bar".to_string(),
        vers: "0.1.0".to_string(),
        checksum: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
        yanked: false,
        entry: sample_entry("bar", "0.1.0", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
    };
    store.put_native_record(&rec).unwrap();

    // Publisher must fail because blob does not exist in store
    let publ = Publisher::new(&store, site.clone(), "http://localhost:8080".to_string());
    let err = publ.publish().unwrap_err();
    let s = format!("{err}");
    assert!(s.contains("missing blob"));
    assert!(!site.join("current").exists());
}

#[test]
fn root_and_prefix_hosting_config() {
    let dir = tempdir().unwrap();
    let store = Store::new(dir.path().join("state"));
    let site = dir.path().join("site");

    let data = b"payload";
    let sha = store.ingest_blob(Cursor::new(data)).unwrap();
    let rec = NativeRecord {
        name: "baz".to_string(),
        vers: "1.2.3".to_string(),
        checksum: sha.clone(),
        yanked: false,
        entry: sample_entry("baz", "1.2.3", &sha),
    };
    store.put_native_record(&rec).unwrap();

    // Test with prefix
    let publ = Publisher::new(
        &store,
        site.clone(),
        "https://deps.example.net/mycrates".to_string(),
    );
    let res = publ.publish().unwrap();

    let gen_dir = site.join("generations").join(&res.generation_id);
    let native_cfg: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(gen_dir.join("registry/config.json")).unwrap()).unwrap();
    assert_eq!(
        native_cfg["dl"],
        "https://deps.example.net/mycrates/blobs/{sha256-checksum}.crate"
    );
    assert_eq!(native_cfg["api"], "https://deps.example.net/mycrates");

    let mirror_cfg: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(gen_dir.join("crates-io/config.json")).unwrap()).unwrap();
    assert_eq!(
        mirror_cfg["dl"],
        "https://deps.example.net/mycrates/blobs/{sha256-checksum}.crate"
    );
    assert!(mirror_cfg.get("api").is_none());
}

#[test]
fn tampered_blob_rejected_before_switch() {
    let dir = tempdir().unwrap();
    let store = Store::new(dir.path().join("state"));
    let site = dir.path().join("site");

    let data = b"original-content";
    let sha = store.ingest_blob(Cursor::new(data)).unwrap();
    let rec = NativeRecord {
        name: "tamper-pkg".to_string(),
        vers: "1.0.0".to_string(),
        checksum: sha.clone(),
        yanked: false,
        entry: sample_entry("tamper-pkg", "1.0.0", &sha),
    };
    store.put_native_record(&rec).unwrap();

    // Tamper with the blob in store
    let blob_path = store.blobs_dir().join(format!("{}.crate", sha));
    fs::write(&blob_path, b"modified-bytes").unwrap();

    let publ = Publisher::new(&store, site.clone(), "http://localhost:8080".to_string());
    let err = publ.publish().unwrap_err();
    let s = format!("{err}");
    assert!(s.contains("tampered blob"));
    assert!(!site.join("current").exists());
}

#[test]
fn staging_recovery() {
    let dir = tempdir().unwrap();
    let store = Store::new(dir.path().join("state"));
    let site = dir.path().join("site");

    fs::create_dir_all(&site).unwrap();
    let stale_staging = site.join(".staging.interrupted.123");
    fs::create_dir_all(&stale_staging).unwrap();
    let stale_tmp = site.join(".current.tmp.interrupted.456");
    fs::write(&stale_tmp, b"temp").unwrap();

    let publ = Publisher::new(&store, site.clone(), "http://localhost:8080".to_string());
    publ.recover_staging().unwrap();

    assert!(!stale_staging.exists());
    assert!(!stale_tmp.exists());
}
