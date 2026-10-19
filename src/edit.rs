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
        let value = c.value(offset);
        left.keyframes.retain(|k| k.time < offset);
        left.keyframes.push(Keyframe {
            time: offset,
            transform: value.clone(),
        });
        right.keyframes = c
            .keyframes
            .iter()
            .filter(|k| k.time > offset)
            .map(|k| Keyframe {
                time: k.time - offset,
                transform: k.transform.clone(),
            })
            .collect();
        right.keyframes.insert(
            0,
            Keyframe {
                time: 0.,
                transform: value,
            },
        );
    }
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
pub fn add_keyframe(c: &mut Clip, local: f64, fps: u32) {
    let t = (local.clamp(0., c.duration) * fps as f64).round() / fps as f64;
    if c.keyframes.len() >= 200 && !c.keyframes.iter().any(|k| (k.time - t).abs() < 1e-8) {
        return;
    }
    let value = c.value(t);
    c.keyframes
        .retain(|k| (k.time - t).abs() > 0.1 / fps as f64);
    c.keyframes.push(Keyframe {
        time: t,
        transform: value,
    });
    c.keyframes.sort_by(|a, b| a.time.total_cmp(&b.time));
}
