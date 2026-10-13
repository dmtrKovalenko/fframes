//! Renders key states of the robot to PNG files for review:
//! `cargo test -p overdrive-example --test look -- --ignored`
use fframes_3d::*;
use overdrive_example::Show;
use overdrive_example::robot::{Pose, RobotState, packed_offset};

fn save(name: &str, pixels: &Pixels) {
    let dir = std::env::temp_dir().join("overdrive");
    std::fs::create_dir_all(&dir).unwrap();
    image::save_buffer(
        dir.join(format!("{name}.png")),
        &pixels.to_straight_rgba(),
        pixels.width,
        pixels.height,
        image::ColorType::Rgba8,
    )
    .unwrap();
}

#[allow(clippy::large_types_passed_by_value)]
fn state(morph: f32, pose: Pose, root: Mat4) -> RobotState {
    RobotState {
        car: Mat4::IDENTITY,
        root,
        pose,
        morph,
        wheel_spin: 0.0,
        eyes: 8.0,
        reactor: 6.0,
        seams: 4.0,
        headlights: 6.0,
        taillights: 5.0,
        cannon: 0.0,
        charge: 0.0,
        chest_open: 0.0,
    }
}

#[test]
#[ignore = "writes PNGs of the robot for review"]
fn looks() {
    let show = Show::new();
    let renderer = Renderer::new(960, 540).exposure(1.3).bloom(Bloom {
        strength: 0.6,
        radius: 0.5,
        threshold: 1.2,
    });
    let standing = Mat4::from_scale(Vec3::splat(overdrive_example::robot::SCALE));
    let cases: Vec<(&str, RobotState)> = vec![
        ("car", state(0.0, Pose::packed(), packed_offset())),
        (
            "morph_30",
            state(
                0.3,
                Pose::packed().lerp(&Pose::stand(), 0.3),
                packed_offset(),
            ),
        ),
        (
            "morph_60",
            state(
                0.6,
                Pose::packed().lerp(&Pose::stand(), 0.6),
                between(packed_offset(), standing, 0.6),
            ),
        ),
        ("robot", state(1.0, Pose::stand(), standing)),
        ("landing", state(1.0, Pose::landing(), standing)),
        ("aim", state(1.0, Pose::aim(), standing)),
    ];
    for (name, mut s) in cases {
        let mut scene = show.scene.clone();
        let lift = show.robot.ground_offset(&mut scene, s.root, &s.pose);
        if s.morph >= 0.6 {
            s.root = Mat4::from_translation(vec3(0.0, lift, 0.0)) * s.root;
        }
        if name == "aim" {
            s.cannon = 1.0;
            s.charge = 8.0;
        }
        show.robot.apply(&mut scene, &s);
        for (view, az) in [("front", 35.0f32), ("back", 210.0), ("side", 100.0)] {
            let camera = Camera::perspective(35.0).orbit(vec3(0.0, 1.6, 0.0), 8.0, az, 10.0);
            save(
                &format!("{name}_{view}"),
                &renderer.render_pixels(&scene, &camera),
            );
        }
    }
}

/// The transform `t` of the way from `a` to `b`.
fn between(a: Mat4, b: Mat4, t: f32) -> Mat4 {
    let (s0, r0, t0) = a.to_scale_rotation_translation();
    let (s1, r1, t1) = b.to_scale_rotation_translation();
    Mat4::from_scale_rotation_translation(s0.lerp(s1, t), r0.slerp(r1, t), t0.lerp(t1, t))
}
