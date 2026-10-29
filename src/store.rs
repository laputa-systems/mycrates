use crate::error::{Error, Result};
use crate::model::{CrateName, Version};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;

#[derive(Debug)]
pub struct Store {
    pub state_dir: PathBuf,
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

    pub fn lock_path(&self) -> PathBuf {
        self.state_dir.join("lock")
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        fs::create_dir_all(self.blobs_dir())?;
        fs::create_dir_all(self.native_dir())?;
        Ok(())
    }

    pub fn ingest_blob<R: Read>(&self, mut reader: R, _expected_name: &CrateName, _expected_ver: &Version) -> Result<String> {
        self.ensure_dirs()?;
        let mut hasher = Sha256::new();
        let tmp_path = self.blobs_dir().join(".tmp.blob");
        let mut tmp = File::create(&tmp_path)?;
        let mut buf = [0u8; 8192];
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            tmp.write_all(&buf[..n])?;
        }
        tmp.sync_all()?;
        let checksum = format!("{:x}", hasher.finalize_reset());
        let blob_path = self.blobs_dir().join(format!("{}.crate", checksum));
        if blob_path.exists() {
            let _ = fs::remove_file(&tmp_path);
            return Ok(checksum);
        }
        fs::rename(&tmp_path, &blob_path)?;
        Ok(checksum)
    }

    #[cfg(unix)]
    #[allow(unsafe_code)]
    pub fn lock_exclusive(&self) -> Result<File> {
        self.ensure_dirs()?;
        let f = OpenOptions::new().create(true).read(true).write(true).open(self.lock_path())?;
        let fd = f.as_raw_fd();
        let res = unsafe { libc::flock(fd, libc::LOCK_EX) };
        if res != 0 {
            return Err(Error::Operation("lock failed".to_string()));
        }
        Ok(f)
    }

    #[cfg(not(unix))]
    pub fn lock_exclusive(&self) -> Result<File> {
        self.ensure_dirs()?;
        let f = OpenOptions::new().create(true).read(true).write(true).open(self.lock_path())?;
        Ok(f)
    }

    pub fn verify(&self) -> Result<()> {
        let blobs = self.blobs_dir();
        if blobs.exists() {
            for entry in fs::read_dir(&blobs)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("crate") {
                    let name = path.file_name().unwrap().to_string_lossy();
                    if name.starts_with(".tmp") {
                        continue;
                    }
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
                    let _ck = format!("{:x}", hasher.finalize_reset());
                }
            }
        }
        Ok(())
    }
}
