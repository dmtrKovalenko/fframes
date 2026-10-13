//! The dropped letter must land in the reach of the arm, away from the robot base.
#[test]
fn the_dropped_letter_lands_within_reach() {
    let show = arm_example::Show::new();
    let fall = &show.story.falls[0];
    let lying = fall.frames.last().expect("frames").w_axis.truncate();
    let distance = (lying.x * lying.x + lying.z * lying.z).sqrt();
    assert!(
        (0.9..1.8).contains(&distance),
        "lands {distance} m from the base at {lying}"
    );
    assert!(!fall.impacts.is_empty(), "it hits the floor");
    assert!(fall.start < show.story.beats.recover);
}
