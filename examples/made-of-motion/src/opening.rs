//! Opening typography: source-timed rulers, lateral camera moves and selection collapse.

use fframes::{FFramesContext, FontQuery, Frame, Svgr, Transform, animation::Easing};

use crate::{QUESTION, label};

pub(super) fn draw(frame: &mut Frame, ctx: &FFramesContext<'_, '_>) -> Svgr<'static> {
    let n = frame.index;
    if n >= 18 {
        let count = (4 + (n - 13) * 8 / 32).min(12);
        let copy = prefix(count);
        let selection = if matches!(n, 18..=20 | 22) {
            let width = frame
                .text_width(
                    ctx,
                    FontQuery {
                        family: "Inter 24pt",
                        size: 54,
                        weight: 700,
                        ..Default::default()
                    },
                    copy,
                )
                .map_or(450., |width| {
                    width as f32 - 1.25 * copy.chars().count().saturating_sub(1) as f32
                });
            let right = 106.25 + width + 24.;
            // The reference briefly resolves into a typesetting selection, then
            // blinks it off/on once. Keep both end caps outside our adapted copy.
            fframes::svgr!(<g opacity={if n == 18 {0.40} else if n == 22 {0.78} else {0.58}}>
                <path d={format!("M82 444 H{right} M82 628 H{right}")}
                    stroke="#553424" stroke-width="2.3" stroke-dasharray="6 7" fill="none"/>
                <path d={format!("M72 444 H92 M82 444 V628 M72 628 H92 M{} 444 H{} M{right} 444 V628 M{} 628 H{}",right-10.,right+10.,right-10.,right+10.)}
                    stroke="#38241b" stroke-width="6" fill="none"/>
            </g>)
        } else {
            Svgr::empty()
        };
        return fframes::svgr!(<g>{selection}{label(copy,106.25,554.,54.,"#171611",false)}</g>);
    }

    if n >= 14 {
        return collapse(frame);
    }

    let count = match n {
        0..=4 => 1,
        5..=7 => 2,
        8..=12 => 3,
        _ => 4,
    };
    let copy = prefix(count);
    let flash = match n {
        1 => fframes::svgr!(<rect width="1440" height="1080" fill="#ce101c"/>),
        2 => fframes::svgr!(<rect width="1440" height="1080" fill="#e8b900"/>),
        _ => Svgr::empty(),
    };
    let rulers = if n == 2 {
        fframes::svgr!(<path d="M0 108 H1440 M0 957 H1440" fill="none"
            stroke="#654020" stroke-width="8" stroke-dasharray="44 44"/>)
    } else if n >= 3 {
        // These are x-height/baseline rulers, not padding above and below the
        // word. They stay full-width while the oversized type pans through them.
        fframes::svgr!(<path d="M0 444 H1440 M0 620 H1440" fill="none"
            stroke="#633b25" stroke-width="4" stroke-dasharray="44 44"
            stroke-dashoffset={n as f32 * 3.0} opacity="0.86"/>)
    } else {
        Svgr::empty()
    };
    if n == 13 {
        return fframes::svgr!(<g>{rulers}
            <path d={include_str!("opening-type.path")} fill="#211914"/>
        </g>);
    }
    let cursor = match n {
        8 => fframes::svgr!(<rect x="506" y="369" width="934" height="257" fill="#191716"/>),
        9 => fframes::svgr!(<rect x="890" y="369" width="456" height="257" fill="#191716"/>),
        10 => fframes::svgr!(<rect x="1252" y="369" width="9" height="257" fill="#38241b"/>),
        _ => Svgr::empty(),
    };
    fframes::svgr!(<g>
        {flash}{rulers}
        <g transform={frame.animate(fframes::timeline!(
            at 0.208_333 => 0.25,
                animate Transform::translate(421.,0.) => Transform::translate(366.,0.), Easing::Linear,
            at 0.25 => 0.291_667,
                animate Transform::translate(366.,0.) => Transform::translate(-125.,0.), Easing::Linear,
            at 0.291_667 => 0.375,
                animate Transform::translate(-125.,0.) => Transform::translate(-176.,0.), Easing::EaseOut,
            at 0.375 => 0.416_667,
                animate Transform::translate(-176.,0.) => Transform::translate(-500.,0.), Easing::Linear,
            at 0.416_667 => 0.458_333,
                animate Transform::translate(-500.,0.) => Transform::translate(-705.,0.), Easing::EaseOut,
            at 0.458_333 => 0.5,
                animate Transform::translate(-705.,0.) => Transform::translate(-722.,0.), Easing::EaseOut,
        ))}>
            <g transform={Transform { translate_y: if n == 1 {1130.} else {620.}, scale: (6.,if n == 1 {30.} else {6.5}).into(), ..Default::default() }}>
                {label(copy,0.,0.,54.,"#211914",false)}
            </g>
        </g>
        {cursor}
    </g>)
}

fn prefix(count: usize) -> &'static str {
    let end = QUESTION
        .match_indices(' ')
        .nth(count.saturating_sub(1))
        .map_or(QUESTION.len(), |(index, _)| index);
    &QUESTION[..end]
}

fn collapse(frame: &Frame) -> Svgr<'static> {
    // Four source exposures collapse the large lettering into broken scanlines
    // before the small selection resolves. This is geometry, not source footage.
    let (streaks, cursor, blur) = match frame.index {
        14 => (
            "M220 548 L223 516 L240 526 L230 536 L250 550 M281 528 L301 543 M330 526 L344 516 L335 541 M374 523 L365 541 M409 528 L427 544 M465 526 L454 544 M501 529 L511 545 M538 529 L551 543 M578 537 H777 M830 542 L853 530 M881 540 L899 523",
            "M0 0",
            5.,
        ),
        15 => (
            "M144 517 V540 H175 M212 540 H257 M286 540 H302 M329 540 H342 M377 540 H389 M408 540 H430 M500 540 H507 M549 540 H647 M709 540 L723 520 L739 540 H759",
            "M1100 448 H1115 M1107 490 V519 M1107 563 V586",
            4.,
        ),
        16 => (
            "M178 539 H218 M292 539 H303 M325 539 H394 M513 539 H557 M577 539 H587 M628 539 H638 M681 539 H691 M710 539 H733",
            "M905 470 V508 M905 551 V581",
            2.5,
        ),
        _ => (
            "M114 539 H122 M143 539 H161 M221 539 H250 M276 539 H292 M320 539 H350 M377 539 H419 M454 539 H472 M491 539 H528 M571 539 H593 M641 539 H659 M686 539 H704",
            "M793 431 H825 M809 472 V494 M809 550 V590",
            4.,
        ),
    };
    fframes::svgr!(<g>
        <defs>
            <filter id="opening-focus" x="-20%" y="-100%" width="140%" height="300%">
                <feGaussianBlur stdDeviation={blur}/>
            </filter>
        </defs>
        <g filter="url(#opening-focus)" opacity="0.82">
            <path d={streaks} fill="none" stroke="#211914" stroke-width="13" stroke-linecap="round"/>
            <path d={cursor} fill="none" stroke="#38241b" stroke-width="12"/>
        </g>
    </g>)
}
