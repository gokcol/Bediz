use crate::document::Annotation;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

const MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_CLIPS: usize = 4096;
const MAX_LAYERS: usize = 512;
pub(super) const MAX_DURATION_MS: i64 = 100 * 3_600_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Clip {
    pub start: i64,
    pub end: i64,
}

impl Clip {
    pub(super) fn duration(self) -> i64 {
        self.end - self.start
    }

    #[cfg(target_os = "windows")]
    pub(super) fn contains(self, time: i64) -> bool {
        self.start <= time && time < self.end
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Layer {
    pub start: i64,
    pub duration: i64,
    pub reference: String,
    pub annotations: Vec<Annotation>,
}

impl Layer {
    pub(super) fn contains(&self, time: i64) -> bool {
        self.start <= time && time - self.start < self.duration
    }

    #[cfg(target_os = "windows")]
    pub(super) fn frame_document(self) -> Result<crate::document::Document, String> {
        use base64::{Engine, engine::general_purpose::STANDARD};
        let png = STANDARD
            .decode(self.reference)
            .map_err(|error| error.to_string())?;
        let mut reader =
            image::ImageReader::with_format(std::io::Cursor::new(png), image::ImageFormat::Png);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        reader.limits(limits);
        let image = reader
            .decode()
            .map_err(|error| error.to_string())?
            .into_rgba8();
        let document = crate::document::Document {
            image: std::sync::Arc::new(image),
            annotations: self.annotations,
            geometry: Default::default(),
        };
        crate::document::validate_document(&document)?;
        Ok(document)
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Project {
    windows_version: u32,
    pub source: PathBuf,
    pub duration: i64,
    pub clips: Vec<Clip>,
    pub layers: Vec<Layer>,
}

impl Project {
    pub(super) fn new(source: PathBuf, duration: i64) -> Self {
        Self {
            windows_version: 1,
            source,
            duration,
            clips: vec![Clip {
                start: 0,
                end: duration,
            }],
            layers: Vec::new(),
        }
    }

    pub(super) fn length(&self) -> i64 {
        self.clips.iter().map(|clip| clip.duration()).sum()
    }

    pub(super) fn source_time(&self, position: i64) -> i64 {
        let mut position = position.clamp(0, self.length() - 1);
        for clip in &self.clips {
            if position < clip.duration() {
                return clip.start + position;
            }
            position -= clip.duration();
        }
        unreachable!("validated clip spans cover the timeline")
    }

    pub(super) fn output_time(&self, source: i64) -> i64 {
        let mut offset = 0;
        for clip in &self.clips {
            if source < clip.end {
                return offset + (source - clip.start).max(0);
            }
            offset += clip.duration();
        }
        self.length().saturating_sub(1)
    }

    pub(super) fn split(&mut self, index: usize, time: i64) -> bool {
        let Some(clip) = self.clips.get(index).copied() else {
            return false;
        };
        if time <= clip.start || time >= clip.end || self.clips.len() >= MAX_CLIPS {
            return false;
        }
        self.clips[index].end = time;
        self.clips.insert(
            index + 1,
            Clip {
                start: time,
                ..clip
            },
        );
        true
    }

    pub(super) fn trim(&mut self, index: usize, range: Clip) -> bool {
        let Some(clip) = self.clips.get_mut(index) else {
            return false;
        };
        if range.start < clip.start
            || range.end > clip.end
            || range.start >= range.end
            || range == *clip
        {
            return false;
        }
        *clip = range;
        true
    }

    pub(super) fn remove(&mut self, index: usize) -> bool {
        if self.clips.len() < 2 || index >= self.clips.len() {
            return false;
        }
        self.clips.remove(index);
        true
    }

    pub(super) fn add_layer(&mut self, layer: Layer) -> bool {
        if self.layers.len() >= MAX_LAYERS {
            return false;
        }
        self.layers.push(layer);
        true
    }

    pub(super) fn validate(&self) -> Result<(), String> {
        let invalid = self.windows_version != 1
            || !(1..=MAX_DURATION_MS).contains(&self.duration)
            || self.clips.is_empty()
            || self.clips.len() > MAX_CLIPS
            || self.layers.len() > MAX_LAYERS
            || !self
                .clips
                .iter()
                .all(|clip| clip.start >= 0 && clip.start < clip.end && clip.end <= self.duration)
            || self
                .clips
                .windows(2)
                .any(|pair| pair[0].end > pair[1].start)
            || self.layers.iter().any(|layer| {
                layer.start < 0
                    || layer.start >= self.duration
                    || !(1..=self.duration - layer.start).contains(&layer.duration)
                    || layer.reference.is_empty()
                    || layer.reference.len() > 45 * 1024 * 1024
            });
        if invalid {
            return Err("This Bediz video project has an invalid timeline.".into());
        }
        Ok(())
    }

    pub(super) fn load(path: &Path) -> Result<Self, String> {
        let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
            return Err("The video project exceeds the 128 MB limit.".into());
        }
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        let mut project: Self = serde_json::from_reader(BufReader::new(file)).map_err(|_| {
            "This file is not a supported Bediz for Windows video project.".to_string()
        })?;
        project.validate()?;
        if project.source.is_relative() {
            let parent = path
                .parent()
                .ok_or("The video project folder is unavailable.")?;
            project.source = parent.join(&project.source);
        }
        if !project.source.is_file() {
            return Err(format!(
                "Could not find the source video at {}.",
                project.source.display()
            ));
        }
        Ok(project)
    }

    pub(super) fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        let parent = path
            .parent()
            .ok_or("Choose a folder for this video project.")?;
        let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        let mut stored = self.clone();
        if let Ok(relative) = stored.source.strip_prefix(parent) {
            stored.source = relative.to_owned();
        }
        let mut writer = BufWriter::new(file.as_file_mut());
        serde_json::to_writer(&mut writer, &stored).map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;
        drop(writer);
        if file.as_file().metadata().map_err(|e| e.to_string())?.len() > MAX_FILE_BYTES {
            return Err("The video project exceeds the 128 MB limit.".into());
        }
        file.as_file().sync_all().map_err(|e| e.to_string())?;
        file.persist(path).map_err(|e| e.error.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_and_split_keep_source_times_and_round_trip_the_editable_project() {
        let folder = tempfile::tempdir().unwrap();
        let source = folder.path().join("original.mp4");
        fs::write(&source, b"fixture").unwrap();
        let file = folder.path().join("project.bedizvideo");
        let mut project = Project::new(source.clone(), 10_000);
        assert!(project.split(0, 4_000));
        assert!(project.trim(
            0,
            Clip {
                start: 1_000,
                end: 3_000
            }
        ));
        assert!(project.trim(
            1,
            Clip {
                start: 5_000,
                end: 8_000
            }
        ));
        assert!(!project.trim(
            0,
            Clip {
                start: 0,
                end: 5_000
            }
        ));
        assert!(project.add_layer(Layer {
            start: 2_500,
            duration: 3_000,
            reference: "AQ==".into(),
            annotations: vec![],
        }));
        project.save(&file).unwrap();
        let restored = Project::load(&file).unwrap();
        assert_eq!(restored.source, source);
        assert_eq!(restored.length(), 5_000);
        assert_eq!(restored.source_time(1999), 2999);
        assert_eq!(restored.source_time(2000), 5000);
        assert_eq!(restored.output_time(4000), 2000);
        assert!(restored.layers[0].contains(restored.source_time(2400)));
        assert!(!restored.layers[0].contains(restored.source_time(2500)));
        assert!(project.remove(0));
        assert!(!project.remove(0));
    }
}
