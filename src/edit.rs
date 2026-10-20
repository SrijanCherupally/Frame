use crate::model::{Clip, Keyframe, Project, uid};
#[derive(Default)]
pub struct History {
    past: Vec<Project>,
    future: Vec<Project>,
}
impl History {
    pub fn remember(&mut self, p: &Project) {
        self.past.push(p.clone());
        if self.past.len() > 100 {
            self.past.remove(0);
        }
        self.future.clear();
    }
    pub fn undo(&mut self, p: &mut Project) {
        if let Some(old) = self.past.pop() {
            self.future.push(p.clone());
            *p = old;
        }
    }
    pub fn redo(&mut self, p: &mut Project) {
        if let Some(next) = self.future.pop() {
            self.past.push(p.clone());
            *p = next;
        }
    }
    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }
}
pub fn split(p: &mut Project, id: &str, time: f64) -> bool {
    let Some(index) = p.clips.iter().position(|c| c.id == id) else {
        return false;
    };
    let c = &p.clips[index];
    let offset = time - c.start;
    if offset < 1. / p.fps as f64 - 1e-8 || c.duration - offset < 1. / p.fps as f64 - 1e-8 {
        return false;
    }
    let (left, right) = pieces(c, offset);
    p.clips[index] = left;
    p.clips.insert(index + 1, right);
    true
}
fn pieces(c: &Clip, offset: f64) -> (Clip, Clip) {
    let mut left = c.clone();
    left.duration = offset;
    let mut right = c.clone();
    right.id = uid();
    right.start += offset;
    right.source_in += offset * c.speed;
    right.duration -= offset;
    if !c.keyframes.is_empty() {
        left.keyframes = slice_keyframes(c, 0., offset);
        right.keyframes = slice_keyframes(c, offset, c.duration);
    }
    right.reveal_offset += offset;

    (left, right)
}
pub fn remove_range(p: &mut Project, start: f64, end: f64) -> bool {
    if !start.is_finite() || !end.is_finite() || end <= start || start < 0. {
        return false;
    }
    let delta = end - start;
    let mut clips = vec![];
    for c in &p.clips {
        if c.end() <= start {
            clips.push(c.clone());
        } else if c.start >= end {
            let mut c = c.clone();
            c.start -= delta;
            clips.push(c);
        } else {
            if c.start < start {
                let (left, _) = pieces(c, start - c.start);
                if left.duration >= 1. / p.fps as f64 - 1e-8 {
                    clips.push(left);
                }
            }
            if c.end() > end {
                let (_, mut right) = pieces(c, end - c.start);
                right.start -= delta;
                if right.duration >= 1. / p.fps as f64 - 1e-8 {
                    clips.push(right);
                }
            }
        }
    }
    p.clips = clips;
    true
}
/// Insert a boundary without changing the curve or the spatial path.
fn insert_exact(c: &mut Clip, t: f64) {
    c.keyframes.sort_by(|a, b| a.time.total_cmp(&b.time));
    if c.keyframes.iter().any(|k| (k.time - t).abs() < 1e-10) {
        return;
    }
    let value = c.value(t);
    let mut key = Keyframe {
        time: t,
        transform: value,
        ..Keyframe::default()
    };
    if let Some(i) = c
        .keyframes
        .windows(2)
        .position(|p| p[0].time < t && t < p[1].time)
    {
        let (a, b) = (&c.keyframes[i], &c.keyframes[i + 1]);
        let u = (t - a.time) / (b.time - a.time);
        let d = a.domain[0] + (a.domain[1] - a.domain[0]) * u;
        key.easing = a.easing;
        key.domain = [d, a.domain[1]];
        let path = a.path.map(|path| {
            path.split(
                [a.transform.x, a.transform.y],
                [b.transform.x, b.transform.y],
                a.fraction(u),
            )
        });
        if let Some((_, right)) = path {
            key.path = Some(right);
        }
        c.keyframes[i].domain[1] = d;
        if let Some((left, _)) = path {
            c.keyframes[i].path = Some(left);
        }
    }
    c.keyframes.push(key);
    c.keyframes.sort_by(|a, b| a.time.total_cmp(&b.time));
}
pub fn slice_keyframes(c: &Clip, from: f64, to: f64) -> Vec<Keyframe> {
    if c.keyframes.is_empty() {
        return vec![];
    }
    let mut c = c.clone();
    c.keyframes.sort_by(|a, b| a.time.total_cmp(&b.time));
    let first = c.keyframes.first().unwrap().time;
    let last = c.keyframes.last().unwrap().time;
    if from > first && from < last {
        insert_exact(&mut c, from);
    }
    if to > first && to < last {
        insert_exact(&mut c, to);
    }
    let value = c.value(from);
    let keys: Vec<_> = c
        .keyframes
        .into_iter()
        .filter(|k| k.time >= from && k.time <= to)
        .map(|mut k| {
            k.time -= from;
            k
        })
        .collect();
    if keys.is_empty() {
        vec![Keyframe {
            time: 0.,
            transform: value,
            ..Keyframe::default()
        }]
    } else {
        keys
    }
}
pub fn add_keyframe(c: &mut Clip, local: f64, fps: u32) {
    let t = ((local.clamp(0., c.duration) * fps as f64).round() / fps as f64).min(c.duration);
    if c.keyframes.len() >= 200 && !c.keyframes.iter().any(|k| (k.time - t).abs() < 1e-8) {
        return;
    }
    insert_exact(c, t);
}
pub fn reverse_animation(c: &mut Clip) {
    let keys = slice_keyframes(c, 0., c.duration);
    if keys.is_empty() {
        return;
    }
    c.keyframes = keys
        .iter()
        .enumerate()
        .rev()
        .map(|(i, k)| {
            let mut next = Keyframe {
                time: c.duration - k.time,
                transform: k.transform.clone(),
                ..Keyframe::default()
            };
            if i > 0 {
                let prev = &keys[i - 1];
                next.easing = prev.easing.reversed();
                next.domain = [1. - prev.domain[1], 1. - prev.domain[0]];
                next.path = prev.path.map(|path| crate::animation::MotionPath {
                    control1: path.control2,
                    control2: path.control1,
                });
            }
            next
        })
        .collect();
}
/// Copy all transform channels, easing and paths, fitting the receiving clip's duration.
pub fn paste_animation(source: &Clip, target: &mut Clip) {
    target.keyframes = slice_keyframes(source, 0., source.duration);
    let ratio = target.duration / source.duration;
    for key in &mut target.keyframes {
        key.time *= ratio;
    }
    target.transform = source.transform.clone();
    target.reveal = source.reveal;
    target.reveal_duration = source.reveal_duration * ratio;
    target.reveal_offset = source.reveal_offset * ratio;
}

/// Separate source audio for independent J/L cuts without duplicating its sound.
pub fn detach_audio(p: &mut Project, id: &str) -> Option<String> {
    let index = p
        .clips
        .iter()
        .position(|c| c.id == id && c.kind == crate::model::Kind::Video && c.volume > 0.)?;
    if p.clips.len() >= 2000 {
        return None;
    }
    let video = &p.clips[index];
    let audio = Clip {
        kind: crate::model::Kind::Audio,
        asset_id: video.asset_id.clone(),
        name: format!("Audio · {}", video.name),
        track: 3,
        start: video.start,
        duration: video.duration,
        source_in: video.source_in,
        speed: video.speed,
        volume: video.volume,
        transition: if video.transition == crate::model::Transition::Fade {
            crate::model::Transition::Fade
        } else {
            crate::model::Transition::None
        },
        fade_duration: video.fade_duration,
        ..Clip::default()
    };
    let id = audio.id.clone();
    p.clips[index].volume = 0.;
    p.clips.push(audio);
    Some(id)
}
