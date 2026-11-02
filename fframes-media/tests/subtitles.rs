use std::sync::atomic::{AtomicUsize, Ordering};

use fframes_media::{CaptionCue, CaptionTrack, CaptionWord, Subtitles, WordState};
use serde::{Deserialize, Deserializer};

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "effect", rename_all = "kebab-case", deny_unknown_fields)]
enum Highlight {
    Outline { color: String },
    Glow { strength: f32 },
}

#[test]
fn legacy_cue_boundaries_match_borrowed_and_owned_vtt() {
    use fframes_media::{FFramesSubtitles, Vtt};

    fn check<'a, T: FFramesSubtitles<'a>>(track: &T) {
        let cue = track.get_cue_by_time(3000).unwrap().1;
        assert!(T::validate_cue_fitting_frame(3000, cue));
        assert!(track.get_cue_by_time(3001).is_none());
        assert_eq!(track.get_cue_stack(1200, 0).len(), 0);
        assert_eq!(track.get_cue_stack(1199, 1).len(), 0);
        assert_eq!(track.get_cue_stack(1201, 0).len(), 1);
        assert!(track.get_cue_stack(u64::MAX, 1).is_empty());
    }

    let input = "WEBVTT\n\n00:01.200 --> 00:03.000\nKeep the last frame\n";
    let borrowed = Vtt::parse(input).unwrap();
    check(&borrowed);
    check(&borrowed.to_owned());
    let track = CaptionTrack::parse(input).unwrap();
    check(&track);
    assert_eq!(track.active_cues(3.0).count(), 0);
}

#[test]
fn vtt_and_json_share_owned_data_and_preserve_legacy_lookup() {
    use fframes_media::FFramesSubtitles;
    let vtt =
        String::from("\u{feff}WEBVTT\r\n\r\nintro\r\n00:00.500 --> 00:02.000\r\nHello world\r\n");
    let track = Subtitles::parse(&vtt).unwrap();
    drop(vtt);
    assert!(track.is_vtt());
    assert_eq!(track.cues()[0].name.as_deref(), Some("intro"));
    assert_eq!(track.cues()[0].words, []);
    assert!(track.get_cue_by_time(499).is_none());
    assert_eq!(
        track.get_cue_by_time(500).unwrap().1.phrase(),
        "Hello world"
    );
    assert!(track.get_cue_by_time(2000).is_some());
    assert!(track.get_cue_by_time(2001).is_none());
    assert_eq!(track.active_cues(2.0).count(), 0);

    let json = track.to_json_pretty().unwrap();
    let reloaded = Subtitles::parse(&json).unwrap();
    assert_eq!(reloaded.get_whole_text(), ["Hello world"]);
    assert_eq!(reloaded.cues()[0].start, 0.5);
    assert!(!reloaded.is_vtt());

    let long =
        Subtitles::parse("WEBVTT\n\n01:02:03.456 --> 01:02:05.000\nFirst line\nSecond line\n")
            .unwrap();
    assert_eq!(long.cues()[0].start, 3723.456);
    assert_eq!(long.cues()[0].phrase(), "First line\nSecond line");
    assert_eq!(Subtitles::parse("WEBVTT\n\n").unwrap().cues(), []);
    let spacers = Subtitles::parse(concat!(
        "WEBVTT\n\n00:00.000 --> 00:01.000\n\n",
        "00:01.000 --> 00:02.000\n \t\n\n",
        "00:02.000 --> 00:03.000\nVisible\n"
    ))
    .unwrap();
    assert_eq!(spacers.get_whole_text(), ["Visible"]);
    assert_eq!(spacers.cues()[0].start, 2.0);
    for invalid in [
        "WEBVTT\n\n00:60.000 --> 01:01.000\nbad minutes\n",
        "WEBVTT\n\n00:00.000 --> 00:01.000\nok\n\ntruncated cue",
    ] {
        assert!(Subtitles::parse(invalid).is_err());
    }
}

#[test]
fn timed_tokens_preserve_gaps_punctuation_and_overlapping_speakers() {
    let track = CaptionTrack::new([
        CaptionCue::words(
            0.0,
            3.0,
            [
                CaptionWord::new("Hello", 0.0, 0.5),
                CaptionWord::new(",", 0.5, 0.6).space_before(false),
                CaptionWord::new("world", 1.0, 2.0),
            ],
        )
        .speaker("A"),
        CaptionCue::text(1.0, 2.5, "Yes").speaker("B"),
    ])
    .unwrap();
    assert_eq!(track.cues()[0].phrase(), "Hello, world");
    assert_eq!(track.active_cues(1.5).count(), 2);
    assert_eq!(track.active_cues(3.0).count(), 0);
    let words = &track.cues()[0].words;
    assert_eq!(words[0].state_at(0.5), WordState::Spoken);
    assert_eq!(words[2].state_at(0.8), WordState::Upcoming);
    assert_eq!(words[2].progress_at(1.5), 0.5);
    assert_eq!(words[2].state_at(f64::NAN), WordState::Upcoming);
}

#[test]
fn invalid_documents_fail_before_rendering() {
    for (json, field) in [
        (r#"{"cues":[{"start":0,"end":0,"text":"x"}]}"#, "end"),
        (r#"{"cues":[{"start":-1,"end":1,"text":"x"}]}"#, "start"),
        (r#"{"cues":[{"start":0,"end":1,"text":" "}]}"#, "nonempty"),
        (
            r#"{"cues":[{"start":0,"end":1,"words":[{"text":"x","start":0,"end":2}]}]}"#,
            "words[0]",
        ),
        (
            r#"{"cues":[{"start":0,"end":2,"words":[{"text":"a","start":0,"end":1},{"text":"b","start":0.5,"end":2}]}]}"#,
            "words[1].start",
        ),
        (
            r#"{"cues":[{"start":0,"end":1,"text":"x","typo":1}]}"#,
            "typo",
        ),
        (
            r#"{"cues":[{"start":0,"end":1,"text":"x","text":"y"}]}"#,
            "duplicate field",
        ),
        (r#"{"cues":[],"cues":[]}"#, "duplicate field"),
    ] {
        assert!(
            Subtitles::parse(json)
                .unwrap_err()
                .to_string()
                .contains(field),
            "{json}"
        );
        assert!(
            Subtitles::parse_json_document(json).is_err(),
            "dispatch accepted {json}"
        );
    }
    assert!(
        Subtitles::parse_json_document(r#"{"v":"5.7","layers":[]}"#)
            .unwrap()
            .is_none()
    );
    assert!(Subtitles::parse_json_document("[1,2,3]").unwrap().is_none());
    assert!(CaptionTrack::new([CaptionCue::text(f64::NAN, 1.0, "bad")]).is_err());
}

#[test]
fn metadata_decodes_once_across_frames_clones_and_threads_including_failures() {
    static DECODES: AtomicUsize = AtomicUsize::new(0);
    struct Counted(Highlight);
    impl<'de> Deserialize<'de> for Counted {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            DECODES.fetch_add(1, Ordering::SeqCst);
            Highlight::deserialize(d).map(Self)
        }
    }
    #[derive(Deserialize)]
    struct Label {
        effect: String,
    }
    let input = r##"{"cues":[{"start":0,"end":2,"words":[
      {"text":"valid","start":0,"end":1,"metadata":{"effect":"outline","color":"#ffffff"}},
      {"text":"invalid","start":1,"end":2,"metadata":{"effect":"unknown"}}
    ]}]}"##;
    let track = CaptionTrack::parse(input).unwrap();
    assert_eq!(DECODES.load(Ordering::SeqCst), 0);
    // A second schema can be requested independently, without registration.
    assert_eq!(
        track.cues()[0].words[0]
            .metadata
            .typed::<Label>()
            .unwrap()
            .effect,
        "outline"
    );
    assert_eq!(DECODES.load(Ordering::SeqCst), 0);
    let clone = track.clone();
    assert!(
        std::ptr::eq(track.cues(), clone.cues()),
        "cloning must share cue storage"
    );
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let cloned = track.clone();
            scope.spawn(move || {
                for _frame in 0..120 {
                    let words = &cloned.cues()[0].words;
                    assert!(matches!(
                        words[0].metadata.typed::<Counted>().unwrap().0,
                        Highlight::Outline { .. }
                    ));
                    assert!(words[1].metadata.typed::<Counted>().is_none());
                    assert_eq!(
                        words[1].metadata.typed::<Label>().unwrap().effect,
                        "unknown"
                    );
                    assert!(words[0].metadata.typed::<Highlight>().is_some());
                }
            });
        }
    });
    assert_eq!(
        DECODES.load(Ordering::SeqCst),
        2,
        "one successful and one failed decode"
    );
    assert!(std::ptr::eq(
        track.cues()[0].words[0]
            .metadata
            .typed::<Counted>()
            .unwrap(),
        clone.cues()[0].words[0]
            .metadata
            .typed::<Counted>()
            .unwrap(),
    ));
    assert!(std::ptr::eq(track.cues(), clone.cues()));
    assert_eq!(DECODES.load(Ordering::SeqCst), 2);
}

#[test]
fn metadata_edits_detach_the_cache_without_changing_other_consumers() {
    let original = CaptionTrack::parse(
        r#"{"cues":[{"start":0,"end":1,"words":[{"text":"x","start":0,"end":1,"metadata":{"effect":"glow","strength":0.5}}]}]}"#,
    ).unwrap();
    let old = &original.cues()[0].words[0].metadata;
    assert_eq!(
        old.typed::<Highlight>(),
        Some(&Highlight::Glow { strength: 0.5 })
    );
    let mut cues = original.cues().to_vec();
    cues[0].words[0]
        .metadata
        .insert("strength".into(), serde_json::json!(0.8));
    assert_eq!(
        cues[0].words[0].metadata.typed::<Highlight>(),
        Some(&Highlight::Glow { strength: 0.8 })
    );
    let edited = CaptionTrack::new(cues).unwrap();
    assert_eq!(
        edited.cues()[0].words[0].metadata.typed::<Highlight>(),
        Some(&Highlight::Glow { strength: 0.8 })
    );
    assert_eq!(
        old.typed::<Highlight>(),
        Some(&Highlight::Glow { strength: 0.5 })
    );
    assert!(!std::ptr::eq(original.cues(), edited.cues()));
}
