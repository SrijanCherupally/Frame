use crate::{
    media::Asset,
    model::{Project, uid},
};
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use serde::Serialize;
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
pub struct Storage {
    pub root: PathBuf,
    _lock: File,
}
impl Storage {
    pub fn default_root() -> PathBuf {
        std::env::var_os("FRAME_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                directories::ProjectDirs::from("org", "Frame", "Frame").map_or_else(
                    || PathBuf::from(".frame-data"),
                    |d| d.data_local_dir().to_owned(),
                )
            })
    }
    pub fn open(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join("library.lock"))?;
        lock.try_lock_exclusive()
            .context("This media library is already open in another Frame instance")?;
        for name in ["media", "jobs", "projects", "snapshots", "preview"] {
            fs::create_dir_all(root.join(name))?;
        }
        Ok(Self { root, _lock: lock })
    }
    pub fn save(&self, p: &Project) -> Result<()> {
        p.validate()?;
        let path = self.root.join("projects/current.json");
        if path.exists()
            && let Ok(old) = load_project(&path)
        {
            atomic_json(&path.with_file_name("previous.json"), &old)?;
        }
        atomic_json(&path, p)
    }
    pub fn load(&self) -> Result<Option<Project>> {
        let path = self.root.join("projects/current.json");
        if !path.exists() {
            return Ok(None);
        }
        match load_project(&path){Ok(p)=>Ok(Some(p)),Err(_)=>load_project(&path.with_file_name("previous.json")).map(Some).context("Project and recovery copy are unreadable. Open a project backup; originals remain untouched.")}
    }
    pub fn snapshot(&self, p: &Project) -> Result<PathBuf> {
        p.validate()?;
        let path = self
            .root
            .join("snapshots")
            .join(format!("{}.frame.json", uid()));
        atomic_json(&path, p)?;
        Ok(path)
    }
    pub fn snapshots(&self) -> Vec<(String, PathBuf)> {
        let mut out = vec![];
        if let Ok(files) = fs::read_dir(self.root.join("snapshots")) {
            for file in files.flatten() {
                if let Ok(p) = load_project(&file.path()) {
                    out.push((p.name, file.path()));
                }
            }
        }
        out.reverse();
        out
    }
    pub fn assets(&self) -> HashMap<String, Asset> {
        let mut result = HashMap::new();
        if let Ok(entries) = fs::read_dir(self.root.join("media")) {
            for entry in entries.flatten() {
                let path = entry.path().join("meta.json");
                if let Ok(a) = fs::read(&path).and_then(|bytes| {
                    serde_json::from_slice::<Asset>(&bytes).map_err(std::io::Error::other)
                }) {
                    result.insert(a.id.clone(), a);
                }
            }
        }
        result
    }
    pub fn asset_dir(&self, id: &str) -> Result<PathBuf> {
        ensure!(crate::model::safe_id(id), "Invalid media ID");
        Ok(self.root.join("media").join(id))
    }
}
pub fn load_project(path: &Path) -> Result<Project> {
    crate::document::project_from_file(path)
}

pub fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    ensure!(
        bytes.len() <= 32 * 1024 * 1024,
        "Project or metadata exceeds 32 MB"
    );
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".{}.tmp", uid()));
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)?;
        #[cfg(unix)]
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
