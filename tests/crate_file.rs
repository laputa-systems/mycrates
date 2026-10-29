use mycrates::crate_file::{scan_crate, ScanLimits};
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::Write;
use tar::Builder;

fn make_crate(name: &str, ver: &str) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    let mut tar = Builder::new(&mut encoder);
    let cargo_toml = format!("[package]\nname = \"{}\"\nversion = \"{}\"\nedition = \"2021\"\n", name, ver);
    let mut header = tar::Header::new_gnu();
    let path = format!("{}-{}/Cargo.toml", name, ver);
    header.set_path(&path).unwrap();
    header.set_size(cargo_toml.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append(&header, cargo_toml.as_bytes()).unwrap();
    let tar_data = tar.into_inner().unwrap();
    encoder.finish().unwrap()
}

#[test]
fn good_crate() {
    let data = make_crate("foo", "1.0.0");
    let res = scan_crate(&data[..], "foo", "1.0.0", &ScanLimits::default()).unwrap();
    assert!(!res.checksum.is_empty());
}

#[test]
fn bad_prefix() {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    let mut tar = Builder::new(&mut encoder);
    let cargo_toml = "[package]\nname = \"foo\"\nversion = \"1.0.0\"\nedition = \"2021\"\n";
    let mut header = tar::Header::new_gnu();
    header.set_path("wrong/Cargo.toml").unwrap();
    header.set_size(cargo_toml.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append(&header, cargo_toml.as_bytes()).unwrap();
    let tar_data = tar.into_inner().unwrap();
    let data = encoder.finish().unwrap();
    let err = scan_crate(&data[..], "foo", "1.0.0", &ScanLimits::default()).unwrap_err();
    let s = format!("{err}");
    assert!(s.contains("integrity"));
}
