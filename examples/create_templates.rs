//! Regenerate the editable, media-free example projects shipped with Frame.
use frame_studio::{
    document::{Document, Workspace},
    model::Project,
    presets::SCENES,
};
use std::{collections::HashMap, path::Path};
fn main() -> anyhow::Result<()> {
    std::fs::create_dir_all("examples/projects")?;
    for &(scene, name, _) in SCENES {
        let p = Project {
            name: name.into(),
            clips: scene.clips(0., 30),
            ..Project::default()
        };
        let workspace = Workspace {
            playhead: 1.,
            zoom: 70.,
            selected: p.clips.first().map(|c| c.id.clone()),
        };
        Document::capture(&p, &HashMap::new(), workspace).save(
            &Path::new("examples/projects")
                .join(format!("{}.frame", name.to_lowercase().replace(' ', "-"))),
        )?;
    }
    Ok(())
}
