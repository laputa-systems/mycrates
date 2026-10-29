use crate::error::{Error, Result};
use crate::model::{CrateName, IndexEntry, Version};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TMP_COUNTER: AtomicU64 = AtomicU64::new(1);

pub struct StoreLockGuard(File);

impl Drop for StoreLockGuard {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

#[derive(Debug, Clone)]
pub struct Store {
    pub state_dir: PathBuf,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NativeRecord {
    pub name: String,
    pub vers: String,
    pub checksum: String,
    pub yanked: bool,
    pub entry: IndexEntry,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpstreamRecord {
    pub name: String,
    pub vers: String,
    pub checksum: String,
    pub raw_index_line: String,
    pub acquired_at: u64,
}

impl Store {
    pub fn new(state_dir: PathBuf) -> Self {
        Self { state_dir }
    }

    pub fn blobs_dir(&self) -> PathBuf {
        self.state_dir.join("blobs")
    }

    pub fn native_dir(&self) -> PathBuf {
        self.state_dir.join("native")
    }

    pub fn upstream_dir(&self) -> PathBuf {
        self.state_dir.join("upstream")
    }

    pub fn pins_dir(&self) -> PathBuf {
        self.state_dir.join("pins")
    }

    pub fn lock_path(&self) -> PathBuf {
        self.state_dir.join("lock")
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        fs::create_dir_all(self.blobs_dir())?;
        fs::create_dir_all(self.native_dir())?;
        fs::create_dir_all(self.upstream_dir())?;
        fs::create_dir_all(self.pins_dir())?;
        Ok(())
    }

    pub fn lock_exclusive(&self) -> Result<StoreLockGuard> {
        self.ensure_dirs()?;
        let f = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(self.lock_path())?;
        f.lock().map_err(|e| Error::Operation(format!("store lock failed: {e}")))?;
        Ok(StoreLockGuard(f))
    }

    pub fn try_lock_exclusive(&self) -> Result<StoreLockGuard> {
        self.ensure_dirs()?;
        let f = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(self.lock_path())?;
        f.try_lock().map_err(|e| Error::Operation(format!("store try_lock failed: {e}")))?;
        Ok(StoreLockGuard(f))
    }

    pub fn recover_staging(&self) -> Result<()> {
        let blobs = self.blobs_dir();
        if blobs.exists() {
            for entry in fs::read_dir(&blobs)? {
                let entry = entry?;
                let file_name = entry.file_name();
                let name = file_name.to_string_lossy();
                if name.starts_with(".tmp") {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
        Ok(())
    }

    fn check_name_collision(&self, dir: &Path, name: &CrateName) -> Result<()> {
        if !dir.exists() {
            return Ok(());
        }
        let norm = name.norm_key();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let existing_dir_name = entry.file_name().to_string_lossy().to_string();
                if let Ok(existing_crate) = CrateName::new(&existing_dir_name) {
                    if existing_crate.norm_key() == norm && existing_crate.as_str() != name.as_str() {
                        return Err(Error::Conflict(format!(
                            "crate name collision: '{}' conflicts with existing '{}'",
                            name.as_str(),
                            existing_crate.as_str()
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn ingest_blob<R: Read>(&self, mut reader: R) -> Result<String> {
        self.ensure_dirs()?;
        let pid = std::process::id();
        let cnt = TMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp_name = format!(".tmp.{}.{}.{}.crate", pid, nanos, cnt);
        let tmp_path = self.blobs_dir().join(&tmp_name);

        let mut hasher = Sha256::new();
        let mut tmp_file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&tmp_path)?;

        let mut buf = [0u8; 8192];
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            tmp_file.write_all(&buf[..n])?;
        }
        tmp_file.sync_all()?;
        drop(tmp_file);

        let checksum = format!("{:x}", hasher.finalize_reset());
        let final_path = self.blobs_dir().join(format!("{}.crate", checksum));

        if final_path.exists() {
            let _ = fs::remove_file(&tmp_path);
            return Ok(checksum);
        }

        fs::rename(&tmp_path, &final_path)?;
        Ok(checksum)
    }

    pub fn put_native_record(&self, record: &NativeRecord) -> Result<bool> {
        self.ensure_dirs()?;
        let name = CrateName::new(&record.name)?;
        let ver = Version::new(&record.vers)?;

        self.check_name_collision(&self.native_dir(), &name)?;

        let crate_dir = self.native_dir().join(name.lower());
        fs::create_dir_all(&crate_dir)?;

        let record_path = crate_dir.join(format!("{}.toml", ver.key()));
        if record_path.exists() {
            let existing_data = fs::read_to_string(&record_path)?;
            let existing: NativeRecord = toml::from_str(&existing_data)
                .map_err(|e| Error::Integrity(format!("invalid native record {}: {e}", record_path.display())))?;
            if existing.checksum == record.checksum {
                return Ok(false);
            } else {
                return Err(Error::Conflict(format!(
                    "version {} already exists with different checksum",
                    record.vers
                )));
            }
        }

        let s = toml::to_string_pretty(record).map_err(|e| Error::Operation(e.to_string()))?;
        let tmp_path = crate_dir.join(format!(".tmp.{}.toml", ver.key()));
        fs::write(&tmp_path, s)?;
        fs::rename(&tmp_path, &record_path)?;
        Ok(true)
    }

    pub fn set_native_yank(&self, name: &str, vers: &str, yanked: bool) -> Result<()> {
        let crate_name = CrateName::new(name)?;
        let version = Version::new(vers)?;
        let record_path = self
            .native_dir()
            .join(crate_name.lower())
            .join(format!("{}.toml", version.key()));
        if !record_path.exists() {
            return Err(Error::Input(format!("crate {} version {} not found", name, vers)));
        }
        let data = fs::read_to_string(&record_path)?;
        let mut record: NativeRecord = toml::from_str(&data)?;
        record.yanked = yanked;
        record.entry.yanked = yanked;
        let s = toml::to_string_pretty(&record).map_err(|e| Error::Operation(e.to_string()))?;
        let tmp_path = record_path.with_extension("tmp.toml");
        fs::write(&tmp_path, s)?;
        fs::rename(&tmp_path, &record_path)?;
        Ok(())
    }

    pub fn put_upstream_record(&self, record: &UpstreamRecord) -> Result<bool> {
        self.ensure_dirs()?;
        let name = CrateName::new(&record.name)?;
        let ver = Version::new(&record.vers)?;

        self.check_name_collision(&self.upstream_dir(), &name)?;

        let crate_dir = self.upstream_dir().join(name.lower());
        fs::create_dir_all(&crate_dir)?;

        let record_path = crate_dir.join(format!("{}.json", ver.key()));
        if record_path.exists() {
            let existing_data = fs::read_to_string(&record_path)?;
            let existing: UpstreamRecord = serde_json::from_str(&existing_data)
                .map_err(|e| Error::Integrity(format!("invalid upstream record {}: {e}", record_path.display())))?;
            if existing.checksum == record.checksum {
                return Ok(false);
            } else {
                return Err(Error::Conflict(format!(
                    "upstream version {} already exists with different checksum",
                    record.vers
                )));
            }
        }

        let s = serde_json::to_string_pretty(record).map_err(|e| Error::Operation(e.to_string()))?;
        let tmp_path = crate_dir.join(format!(".tmp.{}.json", ver.key()));
        fs::write(&tmp_path, s)?;
        fs::rename(&tmp_path, &record_path)?;
        Ok(true)
    }

    pub fn load_native_records(&self) -> Result<Vec<NativeRecord>> {
        let mut list = Vec::new();
        let dir = self.native_dir();
        if !dir.exists() {
            return Ok(list);
        }
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                for rec_entry in fs::read_dir(entry.path())? {
                    let rec_entry = rec_entry?;
                    let path = rec_entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("toml")
                        && !path.file_name().unwrap().to_string_lossy().starts_with(".tmp")
                    {
                        let s = fs::read_to_string(&path)?;
                        let rec: NativeRecord = toml::from_str(&s).map_err(|e| {
                            Error::Integrity(format!("failed to parse native record {}: {e}", path.display()))
                        })?;
                        list.push(rec);
                    }
                }
            }
        }
        list.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.vers.cmp(&b.vers)));
        Ok(list)
    }

    pub fn load_upstream_records(&self) -> Result<Vec<UpstreamRecord>> {
        let mut list = Vec::new();
        let dir = self.upstream_dir();
        if !dir.exists() {
            return Ok(list);
        }
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                for rec_entry in fs::read_dir(entry.path())? {
                    let rec_entry = rec_entry?;
                    let path = rec_entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("json")
                        && !path.file_name().unwrap().to_string_lossy().starts_with(".tmp")
                    {
                        let s = fs::read_to_string(&path)?;
                        let rec: UpstreamRecord = serde_json::from_str(&s).map_err(|e| {
                            Error::Integrity(format!("failed to parse upstream record {}: {e}", path.display()))
                        })?;
                        list.push(rec);
                    }
                }
            }
        }
        list.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.vers.cmp(&b.vers)));
        Ok(list)
    }

    pub fn verify(&self) -> Result<()> {
        let blobs = self.blobs_dir();
        if blobs.exists() {
            for entry in fs::read_dir(&blobs)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("crate") {
                    let fname = path.file_name().unwrap().to_string_lossy();
                    if fname.starts_with(".tmp") {
                        continue;
                    }
                    let expected_sha = fname.trim_end_matches(".crate");
                    let mut f = File::open(&path)?;
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
                    if actual_sha != expected_sha {
                        return Err(Error::Integrity(format!(
                            "corrupted blob: {} (expected {}, got {})",
                            path.display(),
                            expected_sha,
                            actual_sha
                        )));
                    }
                }
            }
        }

        // Verify native records point to existing valid blobs
        for rec in self.load_native_records()? {
            let blob_path = self.blobs_dir().join(format!("{}.crate", rec.checksum));
            if !blob_path.exists() {
                return Err(Error::Integrity(format!(
                    "missing blob for native crate {} {}: {}",
                    rec.name,
                    rec.vers,
                    blob_path.display()
                )));
            }
        }

        // Verify upstream records point to existing valid blobs
        for rec in self.load_upstream_records()? {
            let blob_path = self.blobs_dir().join(format!("{}.crate", rec.checksum));
            if !blob_path.exists() {
                return Err(Error::Integrity(format!(
                    "missing blob for upstream crate {} {}: {}",
                    rec.name,
                    rec.vers,
                    blob_path.display()
                )));
            }
        }

        Ok(())
    }
}
