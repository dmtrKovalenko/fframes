use std::borrow::Cow;
use std::sync::OnceLock;

use fframes::media::ImageData;
use fframes::{FFramesContext, FontQuery, Frame, Svgr, TextOverflow, svgr};
use serde::Deserialize;

use crate::motion::*;

pub const CARD_WIDTH: u32 = 1241;
pub const CARD_HEIGHT: u32 = 108;
pub const ROW_PITCH: f32 = 124.44;
pub const LIST_X: f32 = 99.56;
pub const LIST_Y: f32 = 232.89;

const WHITE: &str = "#f5f4f6";
const SUBTLE: &str = "#aaa4a6";
const BRASS: &str = "#ffcf91";

#[derive(Debug, Deserialize)]
pub struct PullRequest {
    pub repo: String,
    pub number: u32,
    pub title: String,
    pub author: String,
    pub status: Status,
    pub dusted: bool,
    #[serde(skip)]
    pub card_image: OnceLock<ImageData<'static>>,
    #[serde(skip)]
    full_label: OnceLock<Option<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Open,
    Closed,
    Merged,
}

impl Status {
    fn label(&self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Closed => "Closed",
            Self::Merged => "Merged",
        }
    }
}

fn title<'a>(pr: &'a PullRequest, frame: &mut Frame, ctx: &FFramesContext<'a, '_>) -> &'a str {
    let value = pr.title.as_str();
    if let Some(fitted) = pr.full_label.get() {
        return fitted.as_deref().unwrap_or(value);
    }
    let font = FontQuery {
        family: "Inter 24pt",
        size: 25,
        weight: 700,
        ..Default::default()
    };
    match frame.text_fit(ctx, font, value, 969, TextOverflow::Ellipsis) {
        Some(fitted) => pr
            .full_label
            .get_or_init(|| match fitted {
                Cow::Owned(text) => Some(text),
                Cow::Borrowed(_) => None,
            })
            .as_deref()
            .unwrap_or(value),
        None => value,
    }
}

pub fn card<'a>(
    pr: &'a PullRequest,
    frame: &mut Frame,
    ctx: &FFramesContext<'a, '_>,
    glass: Option<&ImageData<'static>>,
    hover: f32,
) -> Svgr<'a> {
    let title = title(pr, frame, ctx);
    let detail = format!("#{} · {} · {}", pr.number, pr.author, pr.status.label());
    let (badge, color) = if pr.repo == "fframes" {
        ("#839dc9", "#b6d7fc")
    } else {
        ("#bb78bb", "#edb5ed")
    };
    let material = glass.map_or_else(
        || svgr!(<rect x="1" y="1" width={CARD_WIDTH - 2} height={CARD_HEIGHT - 2} rx="20"
                       fill="#b1a6bf" fill-opacity="0.072" stroke="#c4b9bd" stroke-opacity="0.13"/>),
        |skin| svgr!(<image width={CARD_WIDTH} height={CARD_HEIGHT} href={skin.href()} preserveAspectRatio="none"/>),
    );
    svgr!(
        <g>
            {material}
            <rect x="1" y="1" width={CARD_WIDTH - 2} height={CARD_HEIGHT - 2} rx="20"
                  fill="#f3e6de" opacity={hover * 0.067}/>
            <text x="85" y="47" font-size="25" font-weight="700" fill={WHITE}>{title}</text>
            <text x="85" y="80" font-size="20" fill={SUBTLE}>{detail}</text>
            <rect x="1084" y="35" width="90" height="38" rx="19" fill={badge} fill-opacity="0.17"/>
            <text x="1129" y="61" font-size="18.5" font-weight="700" text-anchor="middle" fill={color}>{pr.repo.as_str()}</text>
        </g>
    )
}

pub fn heading(t: f32, total: usize, remaining: usize) -> Svgr<'static> {
    let lost = total - remaining;
    let after = lost > 0;
    let count = remaining.to_string();
    let amount = overview(t);
    let outline = hover(t, 1020.0, 101.0, 152.0, 53.0) * (1.0 - ramp(t, THANOS, THANOS + 0.15));
    let hot = hover(t, 1183.0, 101.0, 153.0, 53.0) * (1.0 - ramp(t, THANOS, THANOS + 0.15));
    let disabled = 1.0 - ramp(t, YES, YES + 0.29) * 0.59;
    let lift = press(t, THANOS) * 2.1;
    let label = if amount > 0.5 {
        "List view"
    } else {
        "Overview"
    };
    let hint = if after { "remaining" } else { "pull requests" };
    let loss = format!("−{lost}");
    svgr!(
        <g>
            <text x="102" y="140" font-size="48" font-weight="700" fill={WHITE} letter-spacing="-1.4">"Pull requests"</text>
            <text x="103" y="184" font-size="22" fill={SUBTLE}>"fframes + fff"</text>
            <circle cx="260" cy="177" r="2.2" fill="#6d6c76"/>
            <text x="280" y="184" font-family="JetBrains Mono" font-size="21" fill={WHITE}>{count}</text>
            <text x="327" y="184" font-size="22" fill={SUBTLE}>{hint}</text>
            {if after {
                svgr!(
                    <g opacity={ramp(t, SNAP + 0.28, SNAP + 0.57)}>
                        <rect x="458" y="159" width="72" height="33" rx="8" fill="#382720"/>
                        <text x="494" y="182" font-size="17" font-family="JetBrains Mono" text-anchor="middle" fill="#f5bc87">{loss}</text>
                    </g>
                )
            } else { Svgr::empty() }}
            <g opacity={disabled}>
                <g transform={format!("translate(0 {})", press(t, OVERVIEW) * 1.8)}>
                    <rect x="1020.5" y="101.5" width="151" height="52" rx="19" fill="#c2bccd" fill-opacity="0.055" stroke="#d3ced7" stroke-opacity="0.13"/>
                    <rect x="1021" y="102" width="150" height="51" rx="18" fill="#ececff" opacity={outline * 0.096}/>
                    <path d="M1037 121h5v5h-5z m9 0h5v5h-5z m-9 9h5v5h-5z m9 0h5v5h-5z"
                          fill="none" stroke="#a4a1af" stroke-width="1.5" stroke-linejoin="round"/>
                    <text x="1063" y="135" font-size="21" fill={WHITE}>{label}</text>
                </g>
                <g transform={format!("translate(0 {lift})")}>
                    <rect x="1183.5" y="101.5" width="152" height="52" rx="19" fill="url(#thanos-button)" stroke="#ecbd8f" stroke-opacity="0.41"/>
                    <rect x="1184" y="102" width="151" height="51" rx="18" fill="#ffbc6d" opacity={hot * 0.16}/>
                    <path d="M1208 118q0 10 10 10-10 0-10 10 0-10-10-10 10 0 10-10Z"
                          fill={BRASS}/>
                    <text x="1227" y="135" font-size="22" font-weight="700" fill={BRASS}>"Thanos"</text>
                </g>
            </g>
        </g>
    )
}

pub fn dialog(t: f32, total: usize, removed: usize) -> Svgr<'static> {
    let out = ramp(t, YES + 0.055, YES + 0.25);
    let enter = ramp(t, THANOS, THANOS + 0.23);
    let visible = enter * (1.0 - out);
    if visible <= 0.0 {
        return Svgr::empty();
    }
    let swap = CONFIRM + 0.09;
    let second = t >= swap;
    let next = ramp(t, swap, swap + 0.20);
    let previous = ramp(t, CONFIRM + 0.035, swap);
    let content_alpha = if second { next } else { 1.0 - previous };
    let slide = if second {
        (1.0 - next) * 10.0
    } else {
        -previous * 7.0
    };
    let panel_y = 332.0 + (1.0 - enter) * 18.0 + out * 5.0;
    let scale = 0.965 + enter * 0.035 - out * 0.02;
    let cancel = hover(t, 746.0, 651.0, 146.0, 60.0);
    let action = hover(t, 907.0, 651.0, 153.0, 60.0);
    let down = 2.7 * press(t, if second { YES } else { CONFIRM });
    let (heading, copy, detail, action_label) = if second {
        (
            "Are you sure?",
            format!("{removed} of {total} pull requests will turn to dust."),
            "There’s no undo.",
            "Yes, snap",
        )
    } else {
        (
            "Perfectly balanced.",
            "As all things should be.".to_owned(),
            "Half of the pull requests will turn to dust.",
            "Snap",
        )
    };

    svgr!(
        <g opacity={visible}>
            <rect width="1440" height="1280" fill="#0b090c" opacity="0.30"/>
            <g transform={format!("translate(720 {panel_y}) scale({scale}) translate(-380 0)")}>
                <rect x="-10" y="13" width="780" height="410" rx="29" fill="#09070a" opacity="0.21"/>
                <rect x="0.5" y="0.5" width="759" height="405" rx="24" fill="url(#dialog)" stroke="#d4c5bf" stroke-opacity="0.27"/>
                <path d="M31 1h698" stroke="#ffc991" opacity={0.15 + action * 0.22}/>
                <g transform={format!("translate(0 {slide})")} opacity={content_alpha}>
                    <rect x="40" y="34" width="52" height="52" rx="13" fill="#3a2b26" stroke="#665044"/>
                    {if second {
                        svgr!(
                            <g stroke={BRASS} fill="none" stroke-width="2.3" stroke-linejoin="round" stroke-linecap="round">
                                <path d="M66 45l14 24a2 2 0 0 1-2 3H54a2 2 0 0 1-2-3Z M66 53v9m0 4v1"/>
                            </g>
                        )
                    } else {
                        svgr!(
                            <g stroke={BRASS} stroke-width="2.3" fill="none" stroke-linecap="round" stroke-linejoin="round">
                                <path d="M54 52h24m-12-6v27m-6 0h12M56 53l-6 10h12Zm20 0-6 10h12Z"/>
                            </g>
                        )
                    }}
                    <text x="39" y="155" font-size="55" font-weight="700" fill={WHITE} letter-spacing="-1.95">{heading}</text>
                    <text x="42" y="213" font-size="31" fill="#dad5dc">{copy}</text>
                    <text x="43" y="263" font-size="23" fill={SUBTLE}>{detail}</text>
                </g>
                <path d="M1 300h758" stroke="#393640"/>
                <rect x="406" y="320" width="146" height="60" rx="12" fill="#2a2831" stroke="#45424d"/>
                <rect x="406" y="320" width="146" height="60" rx="12" fill="#42404b" stroke="#888390" opacity={cancel}/>
                <text x="479" y="358" font-size="23" font-weight="700" text-anchor="middle" fill={WHITE}>"Cancel"</text>
                <g transform={format!("translate(0 {down})")}>
                    <rect x="567" y="320" width="153" height="60" rx="12" fill="#e4ae78" stroke="#ffd5a5"/>
                    <rect x="567" y="320" width="153" height="60" rx="12" fill="#ffca90" stroke="#ffe0b9" opacity={action}/>
                    <text x="643.5" y="358" font-size="23" font-weight="700" text-anchor="middle" fill="#211411"
                          opacity={content_alpha}>{action_label}</text>
                </g>
            </g>
        </g>
    )
}

pub fn pointer(t: f32) -> Svgr<'static> {
    let opacity = 1.0 - ramp(t, YES + 0.08, YES + 0.36);
    if opacity <= 0.0 {
        return Svgr::empty();
    }
    let (x, y) = cursor(t);
    let down = CLICKS
        .iter()
        .map(|click| press(t, *click))
        .fold(0.0_f32, f32::max);
    let drag = ramp(t, PICKUP - 0.04, PICKUP + 0.05) * (1.0 - ramp(t, DROP, DROP + 0.11));
    let radius = 15.0 - 2.1 * down - 1.7 * drag;
    svgr!(
        <g transform={format!("translate({x} {y})")} opacity={opacity}>
            <circle cy="1.4" r={radius + 1.1} fill="#0d0b0c" fill-opacity="0.32"/>
            <circle r={radius} fill="#faf9fa" fill-opacity="0.93" stroke="#ffffff" stroke-opacity="0.27"/>
        </g>
    )
}
