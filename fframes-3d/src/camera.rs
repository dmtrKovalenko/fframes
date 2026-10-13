use glam::{Mat4, Quat, Vec2, Vec3};

use crate::math::{deg, noise1};

/// A perspective or orthographic camera, three.js `PerspectiveCamera` with `lookAt`.
///
/// A camera is a plain value. Build a new one for each frame:
///
/// ```rust
/// use fframes_3d::{Camera, vec3};
///
/// let t = 1.5;
/// let camera = Camera::perspective(35.0)
///     .orbit(vec3(0.0, 1.0, 0.0), 6.0, 30.0 + t * 20.0, 12.0)
///     .shake(t, 0.02, 7);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// The position in meters.
    pub position: Vec3,
    /// The point that the camera looks at.
    pub target: Vec3,
    /// The up direction before roll. Default +Y.
    pub up: Vec3,
    /// The rotation around the view direction in radians.
    pub roll: f32,
    /// The vertical field of view in degrees.
    pub fov: f32,
    /// The distance of the near clip plane in meters. Default 0.05.
    pub near: f32,
    /// The distance of the far clip plane in meters. Default 500.
    pub far: f32,
    /// `Some(height)` makes the camera orthographic. It then shows `height` meters
    /// vertically.
    pub ortho_height: Option<f32>,
    /// The image shift in normalized device units, as with a shift lens. `1.0` is half the
    /// frame. It frames a subject off center without a turn of the camera.
    pub shift: Vec2,
    /// The depth of field. Default `None`: everything is sharp.
    pub focus: Option<Focus>,
}

/// The depth of field: the distance that is sharp and the blur of everything else.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Focus {
    /// The distance from the camera that is sharp, in meters.
    pub distance: f32,
    /// The blur radius in pixels at 1080 lines, for things infinitely far behind the focus.
    /// Things in front blur by `blur * (focus / depth - 1)`.
    pub blur: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self::perspective(50.0)
    }
}

impl Camera {
    /// A perspective camera at `(0, 0, 5)` that looks at the origin. `fov_degrees` is the
    /// vertical field of view.
    pub fn perspective(fov_degrees: f32) -> Self {
        Self {
            position: Vec3::new(0.0, 0.0, 5.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            roll: 0.0,
            fov: fov_degrees,
            near: 0.05,
            far: 500.0,
            ortho_height: None,
            shift: Vec2::ZERO,
            focus: None,
        }
    }

    /// An orthographic camera at `(0, 0, 5)` that shows `height` meters vertically.
    pub fn orthographic(height: f32) -> Self {
        Self {
            ortho_height: Some(height),
            ..Self::default()
        }
    }

    /// Sets the position in meters.
    pub fn at(self, x: f32, y: f32, z: f32) -> Self {
        self.position(Vec3::new(x, y, z))
    }

    /// Sets the position in meters.
    pub fn position(mut self, position: Vec3) -> Self {
        self.position = position;
        self
    }

    /// Sets the point that the camera looks at.
    pub fn look_at(mut self, target: Vec3) -> Self {
        self.target = target;
        self
    }

    /// Sets the vertical field of view in degrees.
    pub fn fov(mut self, fov_degrees: f32) -> Self {
        self.fov = fov_degrees;
        self
    }

    /// Adds depth of field with the focus at `distance` meters.
    ///
    /// `blur` is the blur radius of the far background in pixels at 1080p. 6 is subtle, 16
    /// looks like a macro lens.
    pub fn focus(mut self, distance: f32, blur: f32) -> Self {
        self.focus = Some(Focus { distance, blur });
        self
    }

    /// Adds depth of field with the focus on `point`, see [`Camera::focus`].
    pub fn focus_on(self, point: Vec3, blur: f32) -> Self {
        let distance = (point - self.position).dot(self.forward()).max(0.05);
        self.focus(distance, blur)
    }

    /// Sets the rotation around the view direction in radians.
    pub fn roll(mut self, radians: f32) -> Self {
        self.roll = radians;
        self
    }

    /// Sets the distances of the near and far clip planes in meters.
    pub fn clip(mut self, near: f32, far: f32) -> Self {
        self.near = near;
        self.far = far;
        self
    }

    /// Puts the camera on a sphere around `target` and looks at the target.
    ///
    /// `distance` is the radius in meters. `azimuth` is the angle around Y in degrees: 0
    /// looks from +Z, 90 from +X. `elevation` is the angle above the horizon in degrees.
    pub fn orbit(mut self, target: Vec3, distance: f32, azimuth: f32, elevation: f32) -> Self {
        let (az, el) = (deg(azimuth), deg(elevation));
        self.target = target;
        self.position =
            target + Vec3::new(az.sin() * el.cos(), el.sin(), az.cos() * el.cos()) * distance;
        self
    }

    /// Adds hand-held shake: smooth noise turns the camera.
    ///
    /// `amount` is about the angle in radians. Cameras with a different `seed` shake
    /// differently. For an impact, give an amount that decays over time.
    pub fn shake(mut self, time: f32, amount: f32, seed: u32) -> Self {
        if amount == 0.0 {
            return self;
        }
        let distance = self.position.distance(self.target);
        let (right, up) = (self.right(), self.up_vector());
        let frequency = 9.0;
        let dx = noise1(seed, time * frequency) + 0.5 * noise1(seed + 17, time * frequency * 2.3);
        let dy =
            noise1(seed + 31, time * frequency) + 0.5 * noise1(seed + 47, time * frequency * 2.1);
        self.target += (right * dx + up * dy) * amount * distance;
        self.roll += noise1(seed + 59, time * frequency * 0.7) * amount * 0.5;
        self
    }

    /// The unit view direction.
    pub fn forward(&self) -> Vec3 {
        (self.target - self.position).normalize_or(Vec3::NEG_Z)
    }

    /// The unit direction to the right of the image.
    pub fn right(&self) -> Vec3 {
        self.forward().cross(self.up).normalize_or(Vec3::X)
    }

    /// The unit up direction of the image, after roll.
    pub fn up_vector(&self) -> Vec3 {
        let forward = self.forward();
        let up = self.right().cross(forward);
        Quat::from_axis_angle(forward, self.roll) * up
    }

    /// The transform from world space to camera space.
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.position, self.target, self.up_vector())
    }

    /// The projection for an image with the width to height ratio `aspect`.
    pub fn projection_matrix(&self, aspect: f32) -> Mat4 {
        let projection = match self.ortho_height {
            Some(height) => {
                let h = height * 0.5;
                let w = h * aspect;
                Mat4::orthographic_rh(-w, w, -h, h, self.near, self.far)
            }
            None => Mat4::perspective_rh(deg(self.fov), aspect, self.near, self.far),
        };
        if self.shift == Vec2::ZERO {
            projection
        } else {
            Mat4::from_translation(self.shift.extend(0.0)) * projection
        }
    }

    /// The transform from world space to clip space.
    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        self.projection_matrix(aspect) * self.view_matrix()
    }

    /// The position of a world point on a `width` x `height` image.
    ///
    /// `x` and `y` are pixels, `z` is the distance in front of the camera in meters. `None`
    /// when the point is behind the camera. Use it to put SVG labels, lines and particles
    /// on 3D objects: `camera.project(scene.world_position(head), 1920.0, 1080.0)`.
    pub fn project(&self, world: Vec3, width: f32, height: f32) -> Option<Vec3> {
        let clip = self.view_projection(width / height) * world.extend(1.0);
        if clip.w <= 1e-6 {
            return None;
        }
        let ndc = clip / clip.w;
        Some(Vec3::new(
            (ndc.x * 0.5 + 0.5) * width,
            (0.5 - ndc.y * 0.5) * height,
            -(self.view_matrix() * world.extend(1.0)).z,
        ))
    }

    /// The size in pixels of an object `world_size` meters across at `world`, on an image
    /// `height` pixels high.
    pub fn pixel_size(&self, world: Vec3, world_size: f32, height: f32) -> f32 {
        if let Some(h) = self.ortho_height {
            world_size / h * height
        } else {
            let depth = (self.view_matrix() * world.extend(1.0)).z.abs().max(1e-4);
            world_size / (2.0 * depth * (deg(self.fov) * 0.5).tan()) * height
        }
    }
}
