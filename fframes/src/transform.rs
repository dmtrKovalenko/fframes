use usvgr::svgtree::{SvgAttributeValue, svgrtypes};

use crate::animation::Animatable;

/// SVG transform attribute that can be used directly
/// as `transform={...}` argument using svgr! macro
///
/// ```no_run
/// use fframes::{Transform, svgr};
///
/// svgr!(
///   <rect
///     transform={
///       Transform {
///         translate_x: 50.0,
///         rotate: 45.0,
///         translate_y: frame.animate(...),
///         ..Default::default()
///       }
///     }
///   />
/// );
/// ```
///
/// Make sure that it doesn't support multiple transforms which is usually can be
/// replaced by the `transform-origin` attribute, but if you need it use the string
/// based version instead.
///
/// Transforms are also animatable, you can define them directly as values in the timeline!
///
///
/// ```no_run
///   transform={
///     frame.animate(fframes::timeline!(
///         at 0.4,
///         animate Transform::translate(0, 0) => Transform::translate(0, 200),
///         &Easing::EaseOut
///     ))
/// }
/// ````
///
/// Default order of transformations: translate -> rotate -> scale -> skewX -> skewY
#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub struct Transform {
    pub translate_x: f64,
    pub translate_y: f64,
    pub rotate: Rotate,
    pub scale: Scale,
    pub skew_x: f64,
    pub skew_y: f64,
}

impl Transform {
    /// Create a new only translation transform
    pub fn translate(x: impl Into<f64>, y: impl Into<f64>) -> Self {
        Transform {
            translate_x: x.into(),
            translate_y: y.into(),
            ..Default::default()
        }
    }

    /// Create a new only rotation transform
    pub fn rotate(angle: impl Into<Rotate>) -> Self {
        Transform {
            rotate: angle.into(),
            ..Default::default()
        }
    }

    /// Create a new only scale transform
    pub fn scale(factor: impl Into<Scale>) -> Self {
        Transform {
            scale: factor.into(),
            ..Default::default()
        }
    }

    /// Create a new only skewX/skewY transformation
    pub fn skew(x: impl Into<f64>, y: impl Into<f64>) -> Self {
        Transform {
            skew_x: x.into(),
            skew_y: y.into(),
            ..Default::default()
        }
    }
}

impl Animatable for Transform {
    fn apply_progress(&self, to: &Self, progress: f32) -> Self {
        Transform {
            translate_x: self.translate_x + (to.translate_x - self.translate_x) * progress as f64,
            translate_y: self.translate_y + (to.translate_y - self.translate_y) * progress as f64,
            rotate: Rotate {
                angle: self.rotate.angle + (to.rotate.angle - self.rotate.angle) * progress as f64,
                origin: self.rotate.origin.or(to.rotate.origin),
            },
            scale: Scale {
                x: self.scale.x + (to.scale.x - self.scale.x) * progress as f64,
                y: self.scale.y + (to.scale.y - self.scale.y) * progress as f64,
            },
            skew_x: self.skew_x + (to.skew_x - self.skew_x) * progress as f64,
            skew_y: self.skew_y + (to.skew_y - self.skew_y) * progress as f64,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub struct Rotate {
    pub angle: f64,
    pub origin: Option<(f64, f64)>,
}

impl From<f32> for Rotate {
    fn from(angle: f32) -> Self {
        Rotate {
            angle: angle as f64,
            origin: None,
        }
    }
}

impl From<f64> for Rotate {
    fn from(angle: f64) -> Self {
        Rotate {
            angle,
            origin: None,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Scale {
    pub x: f64,
    pub y: f64,
}

impl Default for Scale {
    fn default() -> Self {
        Scale { x: 1.0, y: 1.0 }
    }
}

impl From<f64> for Scale {
    fn from(value: f64) -> Self {
        Scale { x: value, y: value }
    }
}

impl From<f32> for Scale {
    fn from(value: f32) -> Self {
        Scale {
            x: value as f64,
            y: value as f64,
        }
    }
}

impl From<(f64, f64)> for Scale {
    fn from((x, y): (f64, f64)) -> Self {
        Scale { x, y }
    }
}

#[inline(never)]
fn multiply(ts1: &svgrtypes::Transform, ts2: &svgrtypes::Transform) -> svgrtypes::Transform {
    svgrtypes::Transform {
        a: ts1.a * ts2.a + ts1.c * ts2.b,
        b: ts1.b * ts2.a + ts1.d * ts2.b,
        c: ts1.a * ts2.c + ts1.c * ts2.d,
        d: ts1.b * ts2.c + ts1.d * ts2.d,
        e: ts1.a * ts2.e + ts1.c * ts2.f + ts1.e,
        f: ts1.b * ts2.e + ts1.d * ts2.f + ts1.f,
    }
}

impl From<Transform> for SvgAttributeValue<'static> {
    fn from(
        Transform {
            translate_x,
            translate_y,
            scale,
            rotate,
            skew_x,
            skew_y,
        }: Transform,
    ) -> Self {
        let mut final_transform = svgrtypes::Transform::default();

        // Apply translate transformation
        if translate_x != 0.0 || translate_y != 0.0 {
            final_transform = multiply(
                &final_transform,
                &svgrtypes::Transform {
                    a: 1.0,
                    b: 0.0,
                    c: 0.0,
                    d: 1.0,
                    e: translate_x,
                    f: translate_y,
                },
            );
        }

        // Apply rotation transformation
        if rotate.angle != 0.0 {
            let rotate_rad = rotate.angle.to_radians();
            let cos_angle = rotate_rad.cos();
            let sin_angle = rotate_rad.sin();

            if let Some((ox, oy)) = rotate.origin {
                final_transform = multiply(
                    &final_transform,
                    &svgrtypes::Transform {
                        a: 1.0,
                        b: 0.0,
                        c: 0.0,
                        d: 1.0,
                        e: ox,
                        f: oy,
                    },
                );

                final_transform = multiply(
                    &final_transform,
                    &svgrtypes::Transform {
                        a: cos_angle,
                        b: sin_angle,
                        c: -sin_angle,
                        d: cos_angle,
                        e: 0.0,
                        f: 0.0,
                    },
                );

                final_transform = multiply(
                    &final_transform,
                    &svgrtypes::Transform {
                        a: 1.0,
                        b: 0.0,
                        c: 0.0,
                        d: 1.0,
                        e: -ox,
                        f: -oy,
                    },
                );
            } else {
                // Simple rotation around origin
                final_transform = multiply(
                    &final_transform,
                    &svgrtypes::Transform {
                        a: cos_angle,
                        b: sin_angle,
                        c: -sin_angle,
                        d: cos_angle,
                        e: 0.0,
                        f: 0.0,
                    },
                );
            }
        }

        // Apply scale transformation
        if scale.x != 1.0 || scale.y != 1.0 {
            final_transform = multiply(
                &final_transform,
                &svgrtypes::Transform {
                    a: scale.x,
                    b: 0.0,
                    c: 0.0,
                    d: scale.y,
                    e: 0.0,
                    f: 0.0,
                },
            );
        }

        // Apply skewX transformation
        if skew_x != 0.0 {
            final_transform = multiply(
                &final_transform,
                &svgrtypes::Transform {
                    a: 1.0,
                    b: 0.0,
                    c: skew_x.to_radians().tan(),
                    d: 1.0,
                    e: 0.0,
                    f: 0.0,
                },
            );
        }

        // Apply skewY transformation
        if skew_y != 0.0 {
            final_transform = multiply(
                &final_transform,
                &svgrtypes::Transform {
                    a: 1.0,
                    b: skew_y.to_radians().tan(),
                    c: 0.0,
                    d: 1.0,
                    e: 0.0,
                    f: 0.0,
                },
            );
        }

        Self::Transform(final_transform)
    }
}

impl std::fmt::Display for Transform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut first = true;

        if self.translate_x != 0.0 || self.translate_y != 0.0 {
            write!(f, "translate({} {})", self.translate_x, self.translate_y)?;
            first = false;
        }

        if self.rotate.angle != 0.0 {
            if !first {
                write!(f, " ")?;
            }
            first = false;

            if let Some((rotate_origin_x, rotate_origin_y)) = self.rotate.origin {
                write!(
                    f,
                    "rotate({} {} {})",
                    self.rotate.angle, rotate_origin_x, rotate_origin_y
                )?;
            } else {
                write!(f, "rotate({})", self.rotate.angle)?;
            }
        }

        if self.scale.x != 1.0 || self.scale.y != 1.0 {
            if !first {
                write!(f, " ")?;
            }
            first = false;

            if self.scale.x == self.scale.y {
                write!(f, "scale({})", self.scale.x)?;
            } else {
                write!(f, "scale({} {})", self.scale.x, self.scale.y)?;
            }
        }

        if self.skew_x != 0.0 {
            if !first {
                write!(f, " ")?;
            }
            first = false;

            write!(f, "skewX({})", self.skew_x)?;
        }

        if self.skew_y != 0.0 {
            if !first {
                write!(f, " ")?;
            }

            write!(f, "skewY({})", self.skew_y)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn transform_translate_only() {
        let transform = Transform {
            translate_x: 10.0,
            translate_y: 20.0,
            ..Default::default()
        };

        let SvgAttributeValue::Transform(t) = transform.into() else {
            panic!("Expected a transform attribute");
        };

        assert_relative_eq!(t.a, 1.0);
        assert_relative_eq!(t.b, 0.0);
        assert_relative_eq!(t.c, 0.0);
        assert_relative_eq!(t.d, 1.0);
        assert_relative_eq!(t.e, 10.0);
        assert_relative_eq!(t.f, 20.0);
    }

    #[test]
    fn transform_scale_only() {
        let transform = Transform {
            scale: Scale::from((2.0, 3.0)),
            ..Default::default()
        };

        let SvgAttributeValue::Transform(t) = transform.into() else {
            panic!("Expected a transform attribute");
        };

        assert_relative_eq!(t.a, 2.0);
        assert_relative_eq!(t.b, 0.0);
        assert_relative_eq!(t.c, 0.0);
        assert_relative_eq!(t.d, 3.0);
        assert_relative_eq!(t.e, 0.0);
        assert_relative_eq!(t.f, 0.0);
    }

    #[test]
    fn transform_rotate_only() {
        let transform = Transform {
            rotate: Rotate::from(90.0),
            ..Default::default()
        };

        let SvgAttributeValue::Transform(t) = transform.into() else {
            panic!("Expected a transform attribute");
        };

        assert_relative_eq!(t.a, 0.0, epsilon = 1e-10);
        assert_relative_eq!(t.b, 1.0);
        assert_relative_eq!(t.c, -1.0);
        assert_relative_eq!(t.d, 0.0, epsilon = 1e-10);
        assert_relative_eq!(t.e, 0.0);
        assert_relative_eq!(t.f, 0.0);
    }

    #[test]
    fn transform_rotate_with_origin() {
        let transform = Transform {
            rotate: Rotate {
                angle: 30.0,
                origin: Some((10.0, 20.0)),
            },
            ..Default::default()
        };

        let SvgAttributeValue::Transform(t) = transform.into() else {
            panic!("Expected a transform attribute");
        };

        assert_relative_eq!(t.a, 0.8660254037844387, epsilon = 1e-10);
        assert_relative_eq!(t.b, 0.49999999999999994, epsilon = 1e-10);
        assert_relative_eq!(t.c, -0.49999999999999994, epsilon = 1e-10);
        assert_relative_eq!(t.d, 0.8660254037844387, epsilon = 1e-10);
        assert_relative_eq!(t.e, 11.339745962155611, epsilon = 1e-10);
        assert_relative_eq!(t.f, -2.3205080756887746, epsilon = 1e-10);
    }

    #[test]
    fn translate_and_scale() {
        let transform = Transform {
            translate_x: 10.0,
            translate_y: 20.0,
            scale: Scale::from((2.0, 3.0)),
            ..Default::default()
        };

        let SvgAttributeValue::Transform(t) = transform.into() else {
            panic!("Expected a transform attribute");
        };

        assert_relative_eq!(t.a, 2.0);
        assert_relative_eq!(t.b, 0.0);
        assert_relative_eq!(t.c, 0.0);
        assert_relative_eq!(t.d, 3.0);
        assert_relative_eq!(t.e, 10.0);
        assert_relative_eq!(t.f, 20.0);
    }

    #[test]
    fn translate_rotate_scale() {
        let transform = Transform {
            translate_x: 10.0,
            translate_y: 20.0,
            rotate: Rotate::from(90.0),
            scale: Scale::from(2.0),
            ..Default::default()
        };

        let SvgAttributeValue::Transform(t) = transform.into() else {
            panic!("Expected a transform attribute");
        };

        // Check that all transformations are applied in the correct order
        // For a point at (0,0), it should be translated to (10,20),
        // then rotated 90 degrees, then scaled by 2
        let x = 0.0;
        let y = 0.0;
        let new_x = t.a * x + t.c * y + t.e;
        let new_y = t.b * x + t.d * y + t.f;

        assert_relative_eq!(new_x, 10.0, epsilon = 1e-10);
        assert_relative_eq!(new_y, 20.0, epsilon = 1e-10);
    }

    #[test]
    fn skew_transformations() {
        let transform = Transform {
            skew_x: 45.0,
            skew_y: 30.0,
            ..Default::default()
        };

        let SvgAttributeValue::Transform(t) = transform.into() else {
            panic!("Expected a transform attribute");
        };

        // Check skew values
        assert_relative_eq!(t.c, 1.0, epsilon = 1e-10);
        assert_relative_eq!(t.b, 0.57735, epsilon = 1e-5);
    }

    #[test]
    fn translate_sclae_skew() {
        let transform = Transform {
            translate_x: 25.,
            translate_y: 215.,
            scale: 2.0.into(),
            skew_x: 45.0,
            ..Default::default()
        };

        let SvgAttributeValue::Transform(t) = transform.into() else {
            panic!("Expected a transform attribute");
        };

        assert_relative_eq!(t.a, 2.0, epsilon = 1e-10);
        assert_relative_eq!(t.b, 0., epsilon = 1e-10);
        assert_relative_eq!(t.c, 1.9999999999999998, epsilon = 1e-10);
        assert_relative_eq!(t.d, 2.0, epsilon = 1e-10);
        assert_relative_eq!(t.e, 25.0, epsilon = 1e-10);
        assert_relative_eq!(t.f, 215.0, epsilon = 1e-10);
    }
}
