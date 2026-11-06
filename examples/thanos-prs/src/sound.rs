use fframes::AudioTimestamp::{Eof, Second};
use fframes::{AudioMap, AudioTrack};

use crate::motion::*;

pub fn tracks() -> AudioMap<'static> {
    let mut tracks = vec![
        AudioTrack::new("room.wav", Second(0.0)..Second(SNAP - 0.14))
            .gain_db(7.5)
            .fade_in(0.13)
            .fade_out(0.34),
        AudioTrack::new("room.wav", Second(SNAP + 0.27)..Second(COMMENTS + 0.59))
            .offset(SNAP + 0.27)
            .gain_db(6.5)
            .fade_in(0.66)
            .fade_out(0.45),
        AudioTrack::new("room.wav", Second(COMMENTS + 0.81)..Second(DURATION))
            .offset(COMMENTS + 0.81)
            .gain_db(-12.0)
            .fade_in(0.36)
            .fade_out(0.34),
        cue("tick.wav", CANCEL_HOVER, -11.0, 0.13),
        cue("riser.wav", SNAP - 1.42, -0.7, 0.23),
        cue("snap.wav", SNAP - 0.007, 1.9, 0.46),
        cue("shock.wav", SNAP + 0.042, -2.0, 0.0),
        cue("dust.wav", SNAP + 0.20, 0.6, -0.22),
        cue("swipe.wav", MAP_RETURN - 0.05, -10.5, -0.10),
        cue("card.wav", COMMENTS + 0.017, 6.3, 0.34),
    ];
    for time in CLICKS {
        tracks.push(cue("tick.wav", time, 0.7, 0.31));
        tracks.push(cue("swipe.wav", time + 0.025, -8.3, -0.14));
    }
    for i in 1..COMMENT_COUNT {
        let side = if i % 2 == 1 { -0.52 } else { 0.52 };
        let gain = -4.8 - i as f32 * 0.37;
        tracks.push(
            cue("card.wav", COMMENTS + comment_entry(i), gain, side)
                .rate(0.91 + 0.2 * random(i as u32 + 34)),
        );
    }
    tracks.into()
}

fn cue(name: &'static str, time: f32, db: f32, pan: f32) -> AudioTrack<'static> {
    AudioTrack::new(name, Second(time)..Eof)
        .gain_db(db)
        .pan(pan)
}
