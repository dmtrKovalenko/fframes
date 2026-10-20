use std::ops::{Add, Mul};

use glam::{FloatExt, Quat, Vec3};

use crate::{NodeId, Scene};

/// How values change between keyframes, the glTF sampler interpolation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    /// The value jumps at each keyframe.
    Step,
    /// A straight line between keyframes. Rotations use slerp.
    Linear,
    /// A Hermite spline. Each keyframe stores three values: in tangent, value, out tangent.
    CubicSpline,
}

/// The keyframe values of one [`Channel`].
#[derive(Debug, Clone)]
pub enum Keyframes {
    Translation(Vec<Vec3>),
    Rotation(Vec<Quat>),
    Scale(Vec<Vec3>),
    /// Morph target weights, `targets` values for each keyframe.
    Weights {
        targets: usize,
        values: Vec<f32>,
    },
}

/// The animation of one property of one node.
#[derive(Debug, Clone)]
pub struct Channel {
    /// The node id in the model, see [`ModelInstance`](crate::ModelInstance).
    pub node: NodeId,
    pub interpolation: Interpolation,
    /// The keyframe times in seconds, in ascending order.
    pub times: Vec<f32>,
    pub keyframes: Keyframes,
}

/// A keyframe animation, three.js `AnimationClip`. [`Model`](crate::Model) loads clips from
/// glTF.
#[derive(Debug, Clone)]
pub struct AnimationClip {
    pub name: String,
    /// The length in seconds.
    pub duration: f32,
    pub channels: Vec<Channel>,
}

impl AnimationClip {
    /// Poses the nodes at `time` seconds, clamped to the clip.
    ///
    /// `weight` blends the pose over the current pose: 1 replaces it. `offset` is added to
    /// each node id, see [`ModelInstance::offset`](crate::ModelInstance::offset). To
    /// crossfade two clips, apply the first with weight 1 and the second with the progress
    /// of the crossfade.
    pub fn apply(&self, scene: &mut Scene, offset: u32, time: f32, weight: f32) {
        let time = time.clamp(0.0, self.duration.max(0.0));
        for channel in &self.channels {
            let id = NodeId(channel.node.0 + offset);
            let Some(key) = Key::locate(&channel.times, time, channel.interpolation) else {
                continue;
            };
            if let Keyframes::Weights { targets, values } = &channel.keyframes {
                let weights: Vec<Option<f32>> = (0..*targets)
                    .map(|target| {
                        key.sample(|i| values.get(i * targets + target).copied(), f32::lerp)
                    })
                    .collect();
                set_weights(scene, id, &weights, weight);
                continue;
            }
            let Some(node) = scene.get_mut(id) else {
                continue;
            };
            match &channel.keyframes {
                Keyframes::Translation(values) => {
                    if let Some(v) = key.sample(|i| values.get(i).copied(), Vec3::lerp) {
                        node.position = node.position.lerp(v, weight);
                    }
                }
                Keyframes::Scale(values) => {
                    if let Some(v) = key.sample(|i| values.get(i).copied(), Vec3::lerp) {
                        node.scale = node.scale.lerp(v, weight);
                    }
                }
                Keyframes::Rotation(values) => {
                    if let Some(q) = key.sample(|i| values.get(i).copied(), Quat::slerp) {
                        node.rotation = node.rotation.slerp(q.normalize(), weight).normalize();
                    }
                }
                Keyframes::Weights { .. } => {}
            }
        }
    }

    /// [`AnimationClip::apply`] with the time wrapped around the clip.
    pub fn apply_looped(&self, scene: &mut Scene, offset: u32, time: f32, weight: f32) {
        let time = if self.duration > 0.0 {
            time.rem_euclid(self.duration)
        } else {
            0.0
        };
        self.apply(scene, offset, time, weight);
    }
}

/// Blends `weights` with `weight` into the morph weights of the mesh `id`. A glTF node with
/// several primitives is a group with a mesh child for each, and they all get them.
fn set_weights(scene: &mut Scene, id: NodeId, weights: &[Option<f32>], weight: f32) {
    let Some(node) = scene.get(id) else {
        return;
    };
    let meshes: Vec<NodeId> = if node.as_mesh().is_some() {
        vec![id]
    } else {
        (node.children().iter().copied())
            .filter(|&child| scene[child].as_mesh().is_some())
            .collect()
    };
    for mesh in meshes {
        let Some(mesh) = scene[mesh].mesh_mut() else {
            continue;
        };
        mesh.morph_weights.resize(weights.len(), 0.0);
        for (out, value) in mesh.morph_weights.iter_mut().zip(weights) {
            if let Some(value) = value {
                *out += (value - *out) * weight;
            }
        }
    }
}

/// The place of a time between two keyframes of a channel.
struct Key {
    interpolation: Interpolation,
    /// The keyframe at or before the time.
    index: usize,
    /// The progress to the next keyframe, `0..1`. It is 0 before the first and after the
    /// last keyframe.
    t: f32,
    /// The time between the two keyframes in seconds.
    dt: f32,
}

impl Key {
    /// Finds the keyframes around `time`. `None` for a channel without keyframes.
    fn locate(times: &[f32], time: f32, interpolation: Interpolation) -> Option<Self> {
        let last = times.len().checked_sub(1)?;
        let next = times.partition_point(|&t| t <= time);
        let (index, t, dt) = if next == 0 || next > last {
            // Before the first and after the last keyframe, the value does not change.
            (next.min(last), 0.0, 0.0)
        } else {
            let dt = times[next] - times[next - 1];
            let t = if dt > 0.0 {
                (time - times[next - 1]) / dt
            } else {
                0.0
            };
            (next - 1, t, dt)
        };
        Some(Self {
            interpolation,
            index,
            t,
            dt,
        })
    }

    /// The value at this key. `get(i)` reads the stored value `i` of the channel, and `mix`
    /// interpolates in a straight line (lerp or slerp).
    fn sample<T>(&self, get: impl Fn(usize) -> Option<T>, mix: fn(T, T, f32) -> T) -> Option<T>
    where
        T: Copy + Mul<f32, Output = T> + Add<Output = T>,
    {
        let (k, t, dt) = (self.index, self.t, self.dt);
        match self.interpolation {
            Interpolation::Step => get(k),
            Interpolation::Linear => {
                let a = get(k)?;
                Some(mix(a, get(k + 1).unwrap_or(a), t))
            }
            Interpolation::CubicSpline => {
                // The values are triplets: in tangent, value, out tangent.
                let v0 = get(k * 3 + 1)?;
                match (get(k * 3 + 2), get(k * 3 + 4), get(k * 3 + 3)) {
                    (Some(out0), Some(v1), Some(in1)) => {
                        Some(hermite(v0, out0 * dt, v1, in1 * dt, t))
                    }
                    _ => Some(v0),
                }
            }
        }
    }
}

/// The cubic Hermite spline from `v0` with tangent `m0` to `v1` with tangent `m1`, at `t`
/// in `0..1`.
fn hermite<T>(v0: T, m0: T, v1: T, m1: T, t: f32) -> T
where
    T: Mul<f32, Output = T> + Add<Output = T>,
{
    let t2 = t * t;
    let t3 = t2 * t;
    v0 * (2.0 * t3 - 3.0 * t2 + 1.0)
        + m0 * (t3 - 2.0 * t2 + t)
        + v1 * (-2.0 * t3 + 3.0 * t2)
        + m1 * (t3 - t2)
}
