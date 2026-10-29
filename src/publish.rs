use crate::error::{Error, Result};
use crate::index::{mirror_config_json, native_config_json, serialize_index_line, shard_for_name};
use crate::model::CrateName;
use crate::store::Store;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::fs as unix_fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const RENDERER_VERSION: u32 = 1;

static STAGING_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub struct PublishResult {
    pub generation_id: String,
    pub is_noop: bool,
}

pub struct Publisher<'a> {
    pub store: &'a Store,
    pub site_dir: PathBuf,
    pub public_base_url: String,
}

impl<'a> Publisher<'a> {
    pub fn new(store: &'a Store, site_dir: PathBuf, public_base_url: String) -> Self {
        Self {
            store,
            site_dir,
            public_base_url,
        }
    }

    pub fn blobs_dir(&self) -> PathBuf {
        self.site_dir.join("blobs")
    }

    pub fn generations_dir(&self) -> PathBuf {
        self.site_dir.join("generations")
    }

    pub fn current_link(&self) -> PathBuf {
        self.site_dir.join("current")
    }

    fn check_no_symlinks_in_path(&self) -> Result<()> {
        if self.site_dir.is_symlink() {
            return Err(Error::Integrity(format!(
                "site dir itself cannot be a symlink: {}",
                self.site_dir.display()
            )));
        }
        if self.site_dir.exists() {
            for entry in fs::read_dir(&self.site_dir)? {
                let entry = entry?;
                let name = entry.file_name();
                if name != "current" && entry.path().is_symlink() {
                    return Err(Error::Integrity(format!(
                        "unexpected symlink in site dir: {}",
                        entry.path().display()
                    )));
                }
            }
        }
        Ok(())
    }

    pub fn recover_staging(&self) -> Result<()> {
        if self.site_dir.exists() {
            for entry in fs::read_dir(&self.site_dir)? {
                let entry = entry?;
                let fname = entry.file_name().to_string_lossy().to_string();
                if fname.starts_with(".staging") || fname.starts_with(".current.tmp") {
                    let path = entry.path();
                    if path.is_dir() {
                        let _ = fs::remove_dir_all(&path);
                    } else {
                        let _ = fs::remove_file(&path);
                    }
                }
            }
        }
        Ok(())
    }

    fn sync_blob(&self, sha256: &str) -> Result<()> {
        let src_blob = self.store.blobs_dir().join(format!("{}.crate", sha256));
        if !src_blob.exists() {
            return Err(Error::Integrity(format!("missing blob in store: {}", sha256)));
        }

        // Rehash blob before publishing
        let mut f = File::open(&src_blob)?;
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 8192];
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let actual_sha = format!("{:x}", hasher.finalize_reset());
        if actual_sha != sha256 {
            return Err(Error::Integrity(format!(
                "tampered blob in store: {} (expected {}, got {})",
                src_blob.display(),
                sha256,
                actual_sha
            )));
        }

        let dst_blob = self.blobs_dir().join(format!("{}.crate", sha256));
        if !dst_blob.exists() {
            if let Err(_) = fs::hard_link(&src_blob, &dst_blob) {
                // If hard link fails (e.g. EXDEV across devices), fall back to copy
                fs::copy(&src_blob, &dst_blob)?;
            }
        }
        Ok(())
    }

    pub fn publish(&self) -> Result<PublishResult> {
        self.check_no_symlinks_in_path()?;
        self.recover_staging()?;

        fs::create_dir_all(&self.site_dir)?;
        fs::create_dir_all(self.blobs_dir())?;
        fs::create_dir_all(self.generations_dir())?;

        // 1. Load records from store
        let native_records = self.store.load_native_records()?;
        let upstream_records = self.store.load_upstream_records()?;

        // 2. Sync and verify all required blobs first
        for rec in &native_records {
            self.sync_blob(&rec.checksum)?;
        }
        for rec in &upstream_records {
            self.sync_blob(&rec.checksum)?;
        }

        // 3. Set up staging directory
        let pid = std::process::id();
        let cnt = STAGING_COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let staging_name = format!(".staging.{}.{}.{}", pid, nanos, cnt);
        let staging_dir = self.site_dir.join(&staging_name);
        fs::create_dir_all(&staging_dir)?;

        let staging_registry = staging_dir.join("registry");
        let staging_mirror = staging_dir.join("crates-io");
        fs::create_dir_all(&staging_registry)?;
        fs::create_dir_all(&staging_mirror)?;

        // Map of (view, rel_path) -> sha256 for canonical generation id computation
        let mut manifest: BTreeMap<(String, String), String> = BTreeMap::new();

        // 4. Render native registry config.json
        let native_cfg = native_config_json(&self.public_base_url);
        let native_cfg_path = staging_registry.join("config.json");
        fs::write(&native_cfg_path, &native_cfg)?;
        let mut h = Sha256::new();
        h.update(native_cfg.as_bytes());
        manifest.insert(("registry".to_string(), "config.json".to_string()), format!("{:x}", h.finalize_reset()));

        // 5. Render mirror config.json
        let mirror_cfg = mirror_config_json(&self.public_base_url);
        let mirror_cfg_path = staging_mirror.join("config.json");
        fs::write(&mirror_cfg_path, &mirror_cfg)?;
        let mut h = Sha256::new();
        h.update(mirror_cfg.as_bytes());
        manifest.insert(("crates-io".to_string(), "config.json".to_string()), format!("{:x}", h.finalize_reset()));

        // 6. Group native records by crate name
        let mut native_by_crate: BTreeMap<String, Vec<&crate::store::NativeRecord>> = BTreeMap::new();
        for rec in &native_records {
            let lower = CrateName::new(&rec.name)?.lower();
            native_by_crate.entry(lower).or_default().push(rec);
        }

        for (crate_lower, mut recs) in native_by_crate {
            // Sort versions by semver
            recs.sort_by(|a, b| {
                let va = semver::Version::parse(&a.vers);
                let vb = semver::Version::parse(&b.vers);
                match (va, vb) {
                    (Ok(a_ver), Ok(b_ver)) => a_ver.cmp(&b_ver),
                    _ => a.vers.cmp(&b.vers),
                }
            });

            let shard = shard_for_name(&crate_lower);
            let out_path = staging_registry.join(&shard);
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }

            let mut lines = Vec::new();
            for r in recs {
                let line = serialize_index_line(&r.entry)?;
                lines.push(line);
            }
            let content = format!("{}\n", lines.join("\n"));
            fs::write(&out_path, &content)?;

            let mut h = Sha256::new();
            h.update(content.as_bytes());
            manifest.insert(("registry".to_string(), shard), format!("{:x}", h.finalize_reset()));
        }

        // 7. Group upstream records by crate name
        let mut upstream_by_crate: BTreeMap<String, Vec<&crate::store::UpstreamRecord>> = BTreeMap::new();
        for rec in &upstream_records {
            let lower = CrateName::new(&rec.name)?.lower();
            upstream_by_crate.entry(lower).or_default().push(rec);
        }

        for (crate_lower, mut recs) in upstream_by_crate {
            recs.sort_by(|a, b| {
                let va = semver::Version::parse(&a.vers);
                let vb = semver::Version::parse(&b.vers);
                match (va, vb) {
                    (Ok(a_ver), Ok(b_ver)) => a_ver.cmp(&b_ver),
                    _ => a.vers.cmp(&b.vers),
                }
            });

            let shard = shard_for_name(&crate_lower);
            let out_path = staging_mirror.join(&shard);
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }

            let mut lines = Vec::new();
            for r in recs {
                lines.push(r.raw_index_line.trim().to_string());
            }
            let content = format!("{}\n", lines.join("\n"));
            fs::write(&out_path, &content)?;

            let mut h = Sha256::new();
            h.update(content.as_bytes());
            manifest.insert(("crates-io".to_string(), shard), format!("{:x}", h.finalize_reset()));
        }

        // 8. Compute canonical generation ID
        let mut canonical_text = format!("v:{}\n", RENDERER_VERSION);
        for ((view, path), sha) in &manifest {
            canonical_text.push_str(&format!("{}|{}|{}\n", view, path, sha));
        }
        let mut h = Sha256::new();
        h.update(canonical_text.as_bytes());
        let full_sha = format!("{:x}", h.finalize_reset());
        let gen_id = full_sha[..24].to_string();

        let target_gen_dir = self.generations_dir().join(&gen_id);

        // 9. Check if current symlink already points to this generation
        if self.current_link().is_symlink() {
            if let Ok(link_target) = fs::read_link(self.current_link()) {
                let expected_rel = Path::new("generations").join(&gen_id);
                if link_target == expected_rel {
                    // No-op detected
                    let _ = fs::remove_dir_all(&staging_dir);
                    return Ok(PublishResult {
                        generation_id: gen_id,
                        is_noop: true,
                    });
                }
            }
        }

        // Move staging to generations/<gen_id> if not already there
        if !target_gen_dir.exists() {
            fs::rename(&staging_dir, &target_gen_dir)?;
        } else {
            let _ = fs::remove_dir_all(&staging_dir);
        }

        // 10. Atomically switch `current` symlink
        let tmp_link_name = format!(".current.tmp.{}.{}.{}", pid, nanos, cnt);
        let tmp_link = self.site_dir.join(&tmp_link_name);
        let rel_target = format!("generations/{}", gen_id);

        unix_fs::symlink(&rel_target, &tmp_link)?;
        fs::rename(&tmp_link, self.current_link())?;

        Ok(PublishResult {
            generation_id: gen_id,
            is_noop: false,
        })
    }
}
