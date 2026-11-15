use super::*;
use crate::{
    DynamicMediaProvider, FFramesContext, FFramesMode, FontFace, FontSource, FontStretch,
    FontStyle, Frame, MediaProvider, TimeBase, VideoSize,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Debug)]
struct Face;
impl FontFace<'_> for Face {
    fn is_monospaced(&self) -> Option<bool> {
        Some(true)
    }
    fn resolve_char_width(&self, _: usize, _: char) -> Option<usize> {
        Some(10)
    }
}

#[derive(Debug, Default)]
struct Fonts(AtomicUsize);
impl<'a> FontSource<'a> for Fonts {
    fn add_font(&mut self, _: String, _: Arc<dyn AsRef<[u8]> + Send + Sync>) {}
    fn resolve_font(
        &'a self,
        _: &str,
        _: u16,
        _: FontStyle,
        _: FontStretch,
    ) -> Option<Box<dyn FontFace<'a> + 'a>> {
        self.0.fetch_add(1, Ordering::Relaxed);
        Some(Box::new(Face))
    }
}

fn context<'a>(
    fonts: Option<&'a Fonts>,
    media: Option<&'a dyn MediaProvider<'a>>,
) -> FFramesContext<'a, 'a> {
    FFramesContext {
        time_base: TimeBase {
            fps: 10,
            sample_rate: 48_000,
        },
        current_video_size: VideoSize {
            width: 320,
            height: 180,
        },
        duration_in_frames: 30,
        mode: FFramesMode::Renderer,
        scenes: None,
        media_source: media,
        font_source: fonts.map(|f| f as &dyn FontSource<'_>),
        abort_signal: None,
    }
}

fn track() -> CaptionTrack {
    CaptionTrack::new([CaptionCue::words(
        0.0,
        2.0,
        [
            CaptionWord::new("Hello", 0.0, 0.5),
            CaptionWord::new("world", 1.0, 1.5),
        ],
    )])
    .unwrap()
}

fn captions() -> Captions {
    Captions::from_media("speech.json", CaptionRect::new(10.0, 20.0, 200.0, 100.0)).font("Test", 10)
}

#[test]
fn frames_and_seeks_reuse_layout_but_new_media_invalidates_it() {
    let fonts = Fonts::default();
    let mut media = DynamicMediaProvider::default().with_subtitles("speech.json", track());
    let component = captions();
    let mut frame = Frame::new(0, 0, 10);
    let initial = component
        .words_at(0.25, &mut frame, &context(Some(&fonts), Some(&media)))
        .unwrap();
    let positions: Vec<_> = initial.iter().map(|w| w.rect).collect();
    assert_eq!(initial[0].state, WordState::Active);
    assert_eq!(initial[0].progress, 0.5);
    drop(initial);
    let measured = fonts.0.load(Ordering::Relaxed);
    assert!(measured > 0);
    for time in [1.25, 0.0, 0.75, 1.5, 0.25] {
        let words = component
            .words_at(time, &mut frame, &context(Some(&fonts), Some(&media)))
            .unwrap();
        assert_eq!(words.iter().map(|w| w.rect).collect::<Vec<_>>(), positions);
    }
    assert_eq!(fonts.0.load(Ordering::Relaxed), measured);
    media = media.with_subtitles(
        "speech.json",
        CaptionTrack::new([CaptionCue::text(0.0, 2.0, "Replaced")]).unwrap(),
    );
    let words = component
        .words_at(0.25, &mut frame, &context(Some(&fonts), Some(&media)))
        .unwrap();
    assert_eq!(words[0].text, "Replaced");
    assert!(fonts.0.load(Ordering::Relaxed) > measured);
}

#[test]
fn highlights_run_only_for_active_words_and_keep_scoped_metadata() {
    #[derive(serde::Deserialize, PartialEq, Debug)]
    #[serde(rename_all = "kebab-case")]
    enum Effect {
        Outline,
    }
    #[derive(serde::Deserialize)]
    struct Style {
        effect: Effect,
    }
    let track = CaptionTrack::parse(
        r#"{"cues":[{"start":0,"end":2,"metadata":{"speaker":"A"},"words":[{"text":"Moon","start":0.5,"end":1,"metadata":{"effect":"outline"}}]}]}"#,
    ).unwrap();
    let media = DynamicMediaProvider::default().with_subtitles("speech.json", track);
    let fonts = Fonts::default();
    let ctx = context(Some(&fonts), Some(&media));
    let component = captions();
    for (index, expected_calls) in [(0, 0), (5, 1), (9, 1), (10, 0), (20, 0)] {
        let mut frame = Frame::new(index, index, 10);
        let mut calls = 0;
        component
            .try_highlight_with(&mut frame, &ctx, |scope, content| {
                calls += 1;
                assert_eq!(
                    scope.word.metadata.typed::<Style>().unwrap().effect,
                    Effect::Outline
                );
                assert_eq!(scope.cue_metadata["speaker"], "A");
                assert_eq!(scope.word.rect.width, 40.0);
                assert_eq!((scope.word.rect.x, scope.word.rect.y), (0.0, 0.0));
                assert!(scope.composition_rect.x >= 10.0);
                assert!(scope.composition_rect.y >= 20.0);
                scope.mask_text(content)
            })
            .unwrap();
        assert_eq!(calls, expected_calls);
    }
}

#[test]
fn modes_use_the_selected_clock_and_scope_svg_definitions() {
    let media = DynamicMediaProvider::default().with_subtitles("speech.json", track());
    let fonts = Fonts::default();
    let ctx = context(Some(&fonts), Some(&media));
    let mut definitions = std::collections::HashSet::new();
    let default_component = captions();
    let mut frame = Frame::new(2, 12, 10);
    let mut active = Vec::new();
    default_component.highlight_with(&mut frame, &ctx, |scope, text| {
        active.push(scope.word.text);
        scope.mask_text(text)
    });
    assert_eq!(active, ["Hello"], "the default clock must be scene-local");
    active.clear();
    default_component.highlight_at_with(1.2, &mut frame, &ctx, |scope, text| {
        active.push(scope.word.text);
        assert_eq!(scope.time, 1.2);
        scope.mask_text(text)
    });
    assert_eq!(
        active,
        ["world"],
        "explicit time must override the frame clock"
    );
    let (_, findings) =
        crate::diagnostics::collect(|| default_component.render_at(1.2, &mut frame, &ctx));
    assert_eq!(findings.len(), 0, "{findings:?}");
    for (clock, active) in [
        (CaptionClock::Global, "world"),
        (CaptionClock::Scene, "Hello"),
    ] {
        for mode in [
            CaptionMode::Highlight,
            CaptionMode::Word,
            CaptionMode::Reveal,
            CaptionMode::Karaoke,
        ] {
            let component = captions().clock(clock).mode(mode);
            // The local scene is at 0.2s; the composition is at 1.2s.
            let mut frame = Frame::new(2, 12, 10);
            for _ in 0..2 {
                let mut visible = Vec::new();
                let mut highlighted = Vec::new();
                component
                    .try_render_with(&mut frame, &ctx, |scope, content| {
                        let id = scope.id("paint");
                        assert_eq!(id, scope.id("paint"));
                        assert!(definitions.insert(id), "SVG IDs collided across scopes");
                        visible.push(scope.word.text);
                        if scope.is_active() {
                            highlighted.push(scope.word.text);
                        }
                        content
                    })
                    .unwrap();
                assert_eq!(highlighted, [active]);
                let expected = if mode == CaptionMode::Word {
                    vec![active]
                } else if mode == CaptionMode::Reveal && clock == CaptionClock::Scene {
                    vec!["Hello"]
                } else {
                    vec!["Hello", "world"]
                };
                assert_eq!(visible, expected);
            }
        }
    }
}

#[test]
fn legacy_frame_helpers_keep_the_last_cue_frame() {
    let track =
        CaptionTrack::parse("WEBVTT\n\n00:01.200 --> 00:03.000\nKeep the last frame\n").unwrap();
    let end = Frame::new(90, 90, 30);
    assert_eq!(end.get_subtitle_phrase(&track), Some("Keep the last frame"));
    assert!(end.get_subtitle_cue(&track).is_some());
    assert!(Frame::new(91, 91, 30).get_subtitle_cue(&track).is_none());
    assert_eq!(Frame::new(36, 36, 30).get_cue_stack(&track, 0).len(), 0);
    assert_eq!(track.active_cues(3.0).count(), 0);
}

#[test]
fn missing_fonts_can_recover_and_bad_layout_is_reported() {
    let media = DynamicMediaProvider::default().with_subtitles("speech.json", track());
    let mut frame = Frame::new(0, 0, 10);
    let component = captions();
    assert!(
        component
            .words_at(0.0, &mut frame, &context(None, Some(&media)))
            .is_err()
    );
    let fonts = Fonts::default();
    let ctx = context(Some(&fonts), Some(&media));
    assert_eq!(component.words_at(0.0, &mut frame, &ctx).unwrap().len(), 2);
    let too_small =
        Captions::new(track(), CaptionRect::new(0.0, 0.0, 20.0, 100.0)).font("Test", 10);
    let (_, findings) = crate::diagnostics::collect(|| too_small.render(&mut frame, &ctx));
    assert!(
        findings
            .iter()
            .any(|f| matches!(f, crate::diagnostics::Diagnostic::CaptionError { .. }))
    );
}

crate::include_media_dir!(struct Embedded, "fframes/src/captions/fixtures");

#[test]
fn runtime_and_embedded_subtitles_use_the_same_provider_contract() {
    #[derive(serde::Deserialize)]
    struct Style {
        effect: String,
    }
    let embedded = Embedded::new().unwrap();
    let directory = crate::MediaDirectory::read_folder(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/captions/fixtures"
    ))
    .unwrap();
    let runtime = directory.process_media_source().unwrap();
    for provider in [&embedded as &dyn MediaProvider<'_>, &runtime] {
        let subtitles = provider.resolve_subtitles("speech.json").unwrap();
        assert_eq!(
            subtitles.cues()[0].words[0]
                .metadata
                .typed::<Style>()
                .unwrap()
                .effect,
            "plasma"
        );
        assert_eq!(
            provider.resolve_subtitles("phrases.vtt").unwrap().cues()[0].phrase(),
            "Embedded phrase"
        );
        assert_eq!(provider.get_all_subtitle_data().len(), 2);
    }
    let cloned = runtime.clone();
    assert!(std::ptr::eq(
        runtime.resolve_subtitles("speech.json").unwrap().cues(),
        cloned.resolve_subtitles("speech.json").unwrap().cues()
    ));
}
