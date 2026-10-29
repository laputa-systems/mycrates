use crate::error::{Error, Result};
use crate::index::{serialize_index_line, shard_for_name};
use crate::model::IndexEntry;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs as unix_fs;
use std::path::{Path, PathBuf};

const RENDERER_VERSION: u32 = 1;

pub fn generation_id_from_entries(entries: &BTreeMap<(String, String), (String, String)>, renderer_version: u32) -> String {
    // entries keyed by (view, path) -> sha256; build canonical list sorted by view,path
    let mut list: Vec<(String, String, String)> = entries
        .iter()
        .map(|((view, path), (_p, sha))| (view.clone(), path.clone(), sha.clone()))
        .collect();
    list.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut canonical = String::new();
    canonical.push_str(&format!("v:{}\n", renderer_version));
    for (view, path, sha) in list {
        canonical.push_str(&format!("{}|{}|{}\n", view, path, sha));
    }
    let mut h = Sha256::new();
    h.update(canonical.as_bytes());
    format!("{:x}", h.finalize_reset())[..24].to_string()
}

pub struct SiteGen {
    pub site_dir: PathBuf,
}

impl SiteGen {
    pub fn new(site_dir: PathBuf) -> Self {
        Self { site_dir }
    }

    pub fn render(&self, native_entries: &[(String, IndexEntry, String)], mirror_entries: &[(String, IndexEntry, String)], public_base_url: &str) -> Result<String> {
        let gen_id = format!("gen-{}", uuid_simple());
        let gen_dir = self.site_dir.join("generations").join(&gen_id);
        fs::create_dir_all(&gen_dir)?;
        // render both views
        let native_root = gen_dir.join("registry");
        let mirror_root = gen_dir.join("crates-io");
        fs::create_dir_all(&native_root)?;
        fs::create_dir_all(&mirror_root)?;

        // group by shard
        use std::collections::BTreeMap as BM;
        let mut native_by_path: BM<String, Vec<String>> = BM::new();
        for (name, entry, _sha) in native_entries {
            let name_lower = name.to_lowercase();
            let shard = shard_for_name(&name_lower);
            let idx_path = format!("{}/{}", shard, name_lower);
            let line = serialize_index_line(entry)?;
            native_by_path.entry(idx_path).or_default().push(line);
        }
        for (p, mut lines) in native_by_path {
            // sort by semver then string - entries already in order of processing; ensure deterministic
            lines.sort();
            let full = native_root.join(&p);
            if let Some(par) = full.parent() {
                fs::create_dir_all(par)?;
            }
            fs::write(&full, lines.join("\n") + "\n")?;
        }

        let mut mirror_by_path: BM<String, Vec<String>> = BM::new();
        for (name, entry, _sha) in mirror_entries {
            let name_lower = name.to_lowercase();
            let shard = shard_for_name(&name_lower);
            let idx_path = format!("{}/{}", shard, name_lower);
            let line = serialize_index_line(entry)?;
            mirror_by_path.entry(idx_path).or_default().push(line);
        }
        for (p, mut lines) in mirror_by_path {
            lines.sort();
            let full = mirror_root.join(&p);
            if let Some(par) = full.parent() {
                fs::create_dir_all(par)?;
            }
            fs::write(&full, lines.join("\n") + "\n")?;
        }

        // config.json
        let native_cfg = crate::index::native_config_json(public_base_url);
        let mirror_cfg = crate::index::mirror_config_json(public_base_url);
        fs::write(native_root.join("config.json"), native_cfg)?;
        fs::write(mirror_root.join("config.json"), mirror_cfg)?;

        // switch current atomically
        let current = self.site_dir.join("current");
        let tmp_link = self.site_dir.join(".current.tmp");
        let _ = fs::remove_file(&tmp_link);
        std::os::unix::fs::symlink(&gen_dir, &tmp_link)?;
        fs::rename(&tmp_link, &current)?;
        Ok(gen_id)
    }
}

fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    format!("{:x}", t)
}
