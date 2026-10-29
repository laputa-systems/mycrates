use mycrates::model::{CrateName, Version};
use mycrates::store::Store;
use tempfile::tempdir;
use std::io::Cursor;

#[test]
fn ingest_same_bytes() {
    let td = tempdir().unwrap();
    let st = Store::new(td.path().to_path_buf());
    let name = CrateName::new("foo").unwrap();
    let ver = Version::new("1.0.0").unwrap();
    let data = b"hello";
    let c1 = st.ingest_blob(&data[..], &name, &ver).unwrap();
    let c2 = st.ingest_blob(&data[..], &name, &ver).unwrap();
    assert_eq!(c1, c2);
}
