use std::borrow::Cow;
use std::sync::OnceLock;

use fframes::media::ImageData;
use fframes::{FFramesContext, FontQuery, Frame, Svgr, TextOverflow, svgr};
use serde::Deserialize;

use crate::motion::*;

pub const CARD_WIDTH: u32 = 1232;
pub const CARD_HEIGHT: u32 = 94;
pub const ROW_PITCH: f32 = 106.0;
pub const LIST_X: f32 = 104.0;
pub const LIST_Y: f32 = 211.0;

const WHITE: &str = "#f0f0f3";
const SUBTLE: &str = "#a1a0aa";
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
    pub const fn color(&self) -> &'static str {
        match self {
            Self::Open => "#80c6a0",
            Self::Closed => "#d78282",
            Self::Merged => "#b9a0e8",
        }
    }

    fn icon(&self) -> Svgr<'static> {
        let color = self.color();
        let branch = match self {
            Self::Open => svgr!(
                <path d="M0 -8V8m12 0V-4L8-8m0 0h4m-4 0v4"/>
                <circle cx="12" cy="10" r="2.4" fill="#222126"/>
            ),
            Self::Closed => svgr!(
                <path d="M0 -8V8m9-11 6 6m0-6-6 6"/>
            ),
            Self::Merged => svgr!(
                <path d="M0 -8V8m12 0v-2a6 6 0 0 0-6-6H0"/>
                <circle cx="12" cy="10" r="2.4" fill="#222126"/>
            ),
        };
        svgr!(
            <g fill="none" stroke={color} stroke-width="1.85" stroke-linecap="round" stroke-linejoin="round">
                {branch}
                <circle cx="0" cy="-10" r="2.4" fill="#222126"/>
                <circle cx="0" cy="10" r="2.4" fill="#222126"/>
            </g>
        )
    }
}

fn title<'a>(pr: &'a PullRequest, frame: &mut Frame, ctx: &FFramesContext<'a, '_>) -> &'a str {
    let value = pr.title.as_str();
    if let Some(fitted) = pr.full_label.get() {
        return fitted.as_deref().unwrap_or(value);
    }
    let font = FontQuery {
        family: "Inter 24pt",
        size: 27,
        weight: 700,
        ..Default::default()
    };
    match frame.text_fit(ctx, font, value, 985, TextOverflow::Ellipsis) {
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
    hover: f32,
) -> Svgr<'a> {
    let title = title(pr, frame, ctx);
    let detail = format!("#{}   ·   {}", pr.number, pr.author);
    let (badge, color) = if pr.repo == "fframes" {
        ("#253241", "#b6cde8")
    } else {
        ("#342c3e", "#d3bde9")
    };
    svgr!(
        <g>
            <rect x="0.5" y="0.5" width={CARD_WIDTH - 1} height={CARD_HEIGHT - 1} rx="16"
                  fill="#212025" fill-opacity="0.93" stroke="#38373e"/>
            <rect x="1" y="1" width={CARD_WIDTH - 2} height={CARD_HEIGHT - 2} rx="15"
                  fill="#faf9ff" opacity={hover * 0.057}/>
            <g transform="translate(35 47)">{pr.status.icon()}</g>
            <text x="71" y="40" font-size="27" font-weight="700" fill={WHITE}>{title}</text>
            <text x="72" y="69" font-size="21" fill={SUBTLE}>{detail}</text>
            <rect x="1101" y="30" width="92" height="34" rx="9" fill={badge}/>
            <text x="1147" y="53" font-size="18" font-weight="700" text-anchor="middle" fill={color}>{pr.repo.as_str()}</text>
            <path d="M1208 42l5 5-5 5" stroke="#73717a" stroke-width="1.5" stroke-linejoin="round"
                  stroke-linecap="round" fill="none"/>
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
            <text x="103" y="133" font-size="58" font-weight="700" fill={WHITE} letter-spacing="-2.25">"Pull requests"</text>
            <text x="106" y="170" font-size="22" fill={SUBTLE}>"fframes + fff"</text>
            <circle cx="260" cy="163" r="2.2" fill="#6d6c76"/>
            <text x="280" y="170" font-family="JetBrains Mono" font-size="21" fill={WHITE}>{count}</text>
            <text x="327" y="170" font-size="22" fill={SUBTLE}>{hint}</text>
            {if after {
                svgr!(
                    <g opacity={ramp(t, SNAP + 0.28, SNAP + 0.57)}>
                        <rect x="458" y="145" width="72" height="33" rx="8" fill="#382720"/>
                        <text x="494" y="168" font-size="17" font-family="JetBrains Mono" text-anchor="middle" fill="#f5bc87">{loss}</text>
                    </g>
                )
            } else { Svgr::empty() }}
            <g opacity={disabled}>
                <g transform={format!("translate(0 {})", press(t, OVERVIEW) * 1.8)}>
                    <rect x="1020.5" y="101.5" width="151" height="52" rx="13" fill="#1e1e23" stroke="#38373f"/>
                    <rect x="1021" y="102" width="150" height="51" rx="12" fill="#ececff" opacity={outline * 0.096}/>
                    <path d="M1037 121h5v5h-5z m9 0h5v5h-5z m-9 9h5v5h-5z m9 0h5v5h-5z"
                          fill="none" stroke="#a4a1af" stroke-width="1.5" stroke-linejoin="round"/>
                    <text x="1063" y="135" font-size="21" fill={WHITE}>{label}</text>
                </g>
                <g transform={format!("translate(0 {lift})")}>
                    <rect x="1183.5" y="101.5" width="152" height="52" rx="13" fill="url(#thanos-button)" stroke="#ad7850"/>
                    <rect x="1184" y="102" width="151" height="51" rx="12" fill="#ffbc6d" opacity={hot * 0.16}/>
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
            <rect width="1440" height="1280" fill="#08070a" opacity="0.66"/>
            <g transform={format!("translate(720 {panel_y}) scale({scale}) translate(-380 0)")}>
                <rect x="-10" y="13" width="780" height="410" rx="29" fill="#050407" opacity="0.48"/>
                <rect x="0.5" y="0.5" width="759" height="405" rx="24" fill="url(#dialog)" stroke="#55515a"/>
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
    let scale = 1.0 - 0.11 * down;
    svgr!(
        <g transform={format!("translate({x} {y}) scale({scale})")} opacity={opacity}>
            <path d="M0 0V31l8.2-8 6 12.8 5.3-2.5-5.9-12.2h11.5Z"
                  transform="translate(1.7 2.5)" fill="#020204" stroke="#020204" stroke-width="4"
                  opacity="0.63" stroke-linejoin="round"/>
            <path d="M0 0V31l8.2-8 6 12.8 5.3-2.5-5.9-12.2h11.5Z"
                  fill="#faf9fc" stroke="#15141b" stroke-width="2.1" stroke-linejoin="round"/>
        </g>
    )
}
