//! Small, portable project documents. Source media is referenced, never embedded.
use crate::{
    media::Asset,
    model::{Project, safe_id, uid},
    storage::{Storage, atomic_json},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    path::Path,
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Workspace {
    pub playhead: f64,
    pub zoom: f32,
    pub selected: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    pub format: String,
    pub version: u32,
    pub project: Project,
    pub media: Vec<Asset>,
    #[serde(default)]
    pub workspace: Workspace,
}
impl Document {
    pub fn capture(
        project: &Project,
        assets: &HashMap<String, Asset>,
        workspace: Workspace,
    ) -> Self {
        let ids: HashSet<_> = project
            .clips
            .iter()
            .filter_map(|c| c.asset_id.as_ref())
            .collect();
        let mut media: Vec<_> = assets
            .values()
            .filter(|a| ids.contains(&a.id))
            .cloned()
            .collect();
        media.sort_by(|a, b| a.id.cmp(&b.id));
        for a in &mut media {
            a.proxy_path = None;
            a.error = None;
            a.status = "linked".into();
        }
        Self {
            format: "frame-project".into(),
            version: 1,
            project: project.clone(),
            media,
            workspace,
        }
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.format == "frame-project" && self.version == 1,
            "Unsupported Frame document version"
        );
        self.project.validate()?;
        ensure!(self.media.len() <= 2000, "Too many media references");
        ensure!(
            self.workspace
                .selected
                .as_ref()
                .is_none_or(|id| id.len() <= 100),
            "Invalid selected clip"
        );
        let mut ids = HashSet::new();
        for a in &self.media {
            ensure!(
                safe_id(&a.id) && ids.insert(&a.id),
                "Invalid or duplicate media reference"
            );
            ensure!(
                a.name.len() <= 1200
                    && !a.path.as_os_str().is_empty()
                    && a.path.as_os_str().len() <= 32768,
                "Media reference too long"
            );
            ensure!(
                a.duration.is_finite()
                    && (0.0..=86400.).contains(&a.duration)
                    && a.fps.is_finite()
                    && a.fps >= 0.,
                "Invalid media metadata"
            );
        }
        ensure!(
            self.workspace.playhead.is_finite()
                && (0.0..=86400.).contains(&self.workspace.playhead),
            "Invalid playhead"
        );
        ensure!(
            self.workspace.zoom.is_finite() && (0.0..=10000.).contains(&self.workspace.zoom),
            "Invalid timeline zoom"
        );
        Ok(())
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let parent = parent
            .canonicalize()
            .context("Project folder is unavailable")?;
        let mut portable = self.clone();
        for a in &mut portable.media {
            a.proxy_path = None;
            a.error = None;
            a.status = "linked".into();
            // Relative references survive moving a project and its adjacent media together.
            if let Ok(relative) = a.path.strip_prefix(&parent) {
                a.path = relative.to_owned();
            }
        }
        atomic_json(path, &portable)
    }
    pub fn read(path: &Path) -> Result<Self> {
        ensure!(
            fs::metadata(path)?.len() <= 32 * 1024 * 1024,
            "Project exceeds 32 MB"
        );
        let mut bytes = vec![];
        fs::File::open(path)?
            .take(32 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 32 * 1024 * 1024, "Project exceeds 32 MB");
        let value: serde_json::Value = serde_json::from_slice(&bytes)?;
        let mut doc: Self = if value.get("format").is_some() {
            serde_json::from_value(value).context("Invalid Frame document")?
        } else {
            ensure!(
                value.get("version").is_some() && value.get("clips").is_some(),
                "This file is not a Frame project"
            );
            let project: Project =
                serde_json::from_value(value).context("Invalid legacy project")?;
            Self::capture(&project, &HashMap::new(), Workspace::default())
        };
        doc.validate()?;
        let parent = path
            .canonicalize()?
            .parent()
            .context("Missing project folder")?
            .to_owned();
        for a in &mut doc.media {
            if a.path.is_relative() {
                a.path = parent.join(&a.path);
            }
        }
        Ok(doc)
    }
    /// Restore references under collision-free local IDs; don't overwrite another project's media.
    pub fn restore(
        &mut self,
        storage: &Storage,
        existing: &HashMap<String, Asset>,
    ) -> Result<Vec<Asset>> {
        self.restore_at(&storage.root, existing)
    }
    /// Background restore using the root already locked by the owning application.
    pub fn restore_at(
        &mut self,
        root: &Path,
        existing: &HashMap<String, Asset>,
    ) -> Result<Vec<Asset>> {
        self.validate()?;
        static RESTORES: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = RESTORES
            .lock()
            .map_err(|_| anyhow::anyhow!("Project restore worker failed"))?;
        let mut restored = vec![];
        let mut remapped_media = vec![];
        for source in &self.media {
            let mut a = source.clone();
            // Read current disk metadata too: another open/import may have completed
            // since the caller captured its library snapshot.
            let local = fs::read(root.join("media").join(&a.id).join("meta.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Asset>(&bytes).ok())
                .or_else(|| existing.get(&a.id).cloned());
            if let Some(local) = local {
                if local.path == a.path || (local.demo && a.demo) {
                    remapped_media.push(local.clone());
                    if !existing.contains_key(&a.id) {
                        restored.push(local);
                    }
                    continue;
                }
                a.id = uid();
                for c in &mut self.project.clips {
                    if c.asset_id.as_deref() == Some(&source.id) {
                        c.asset_id = Some(a.id.clone());
                    }
                }
            }
            a.proxy_path = None;
            a.status = if a.path.is_file() {
                "processing"
            } else {
                "error"
            }
            .into();
            a.error = if a.path.is_file() {
                None
            } else {
                Some("Original is offline. Reconnect its drive or relink this media.".into())
            };
            a.demo = false;
            atomic_json(&root.join("media").join(&a.id).join("meta.json"), &a)?;
            remapped_media.push(a.clone());
            restored.push(a);
        }
        self.media = remapped_media;
        self.project.validate()?;
        Ok(restored)
    }
}
pub fn project_from_file(path: &Path) -> Result<Project> {
    Ok(Document::read(path)?.project)
}
