//! Since 2021: the first commit, five years of history scrolling past, and
//! the numbers behind it.

use std::sync::LazyLock;

use fframes::{Duration, FFramesContext, Frame, Scene, ShaderUniforms, Svgr};

use crate::beat::*;
use crate::shaders::SHADERS;
use crate::ui::*;

beat_scene!(OriginScene, Some(0.0), Some(32.0));

struct Commit {
    hash: &'static str,
    date: &'static str,
    message: &'static str,
}

/// `git log` of the repository, written by build.rs.
static COMMITS: LazyLock<Vec<Commit>> = LazyLock::new(|| {
    include_str!(concat!(env!("OUT_DIR"), "/commits.txt"))
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '|');
            Some(Commit {
                hash: parts.next()?,
                date: parts.next()?,
                message: parts.next()?,
            })
        })
        .collect()
});

/// Numbers derived from the history: commit count and the span from the
/// first commit to the latest one.
struct History {
    count: usize,
    days: i64,
    since: String,
    until: String,
}

static HISTORY: LazyLock<History> = LazyLock::new(|| {
    let first = COMMITS.first().map(|c| c.date).unwrap_or("2021-09-19");
    let last = COMMITS.last().map(|c| c.date).unwrap_or(first);
    History {
        count: COMMITS.len(),
        days: day_number(last) - day_number(first),
        since: month_label(first),
        until: month_label(last),
    }
});

/// Days since 1970-01-01 of a `YYYY-MM-DD` date (Howard Hinnant's algorithm).
fn day_number(date: &str) -> i64 {
    let mut parts = date.split('-').map(|p| p.parse::<i64>().unwrap_or(1));
    let (y, m, d) = (
        parts.next().unwrap_or(1970),
        parts.next().unwrap_or(1),
        parts.next().unwrap_or(1),
    );
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `2021-09-19` as `SEP 2021`.
fn month_label(date: &str) -> String {
    const MONTHS: [&str; 12] = [
        "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
    ];
    let month = date
        .get(5..7)
        .and_then(|m| m.parse::<usize>().ok())
        .unwrap_or(1);
    format!(
        "{} {}",
        MONTHS[(month - 1).min(11)],
        date.get(..4).unwrap_or("")
    )
}

impl Scene for OriginScene {
    fn duration(&self) -> Duration<'_> {
        Self::frames()
    }

    fn render_frame<'a>(&'a self, mut frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let lb = Self::lb(&frame);
        let well_x = 0.7 - prog(lb, 0.0, 32.0) * 0.25;
        let bg = SHADERS.contour.draw(
            &frame,
            ShaderUniforms::new()
                .float2("uWell", well_x, 0.58)
                .float("uDepth", 0.8)
                .float("uDensity", 18.0)
                .float("uBright", 0.42)
                .color("uInk", fframes::Color::hex("#8a857c"))
                .color("uHot", fframes::Color::hex(ORANGE)),
        );

        let t = lb;
        let content = if t < 8.0 {
            first_commit(&mut frame, ctx, t)
        } else if t < 16.0 {
            history(t - 8.0)
        } else {
            stats(&mut frame, ctx, t - 16.0)
        };

        fframes::svgr!(
            <g>
                <image href={bg.href()} x="0" y="0" width="1920" height="1080" />
                {content}
            </g>
        )
    }
}

/// Beats 0-8: the year of the first commit, one digit per beat, then the
/// commit itself.
fn first_commit(frame: &mut Frame, ctx: &FFramesContext, lb: f32) -> Svgr<'static> {
    let Some(first) = COMMITS.first() else {
        return Svgr::empty();
    };
    let year = first.date.get(..4).unwrap_or("2021");
    let years_ago = format!("{} YEARS AGO", HISTORY.days / 365);
    let date_and_author = format!("{}   Dmitriy Kovalenko", first.date);
    let message = format!("“{}”", first.message);
    let exit = expo_in(prog(lb, 7.6, 8.0));
    let command = "$ git log --reverse | head -1";
    let typed = ((lb / 0.8) * command.len() as f32) as usize;
    let command: String = command.chars().take(typed.min(command.len())).collect();

    let year = letters(
        frame,
        ctx,
        150.0,
        690.0,
        year,
        DISPLAY,
        440,
        400,
        BONE,
        -8.0,
        |i| {
            let s = snap(lb - i as f32);
            (
                0.0,
                (1.0 - s) * 160.0,
                prog(lb, i as f32, i as f32 + 0.15),
                0.85 + 0.15 * s,
            )
        },
    );
    let underline = quart_out(prog(lb, 3.0, 4.0)) * 1120.0;
    let meta = prog(lb, 4.0, 4.4);
    let hash_w = 7.0 * 36.0 * 0.6;

    fframes::svgr!(
        <g opacity={1.0 - exit} transform={format!("translate(0 {})", -exit * 120.0)}>
            <text x="160" y="220" font-family={MONO} font-weight="500" font-size="30" fill={GREY}>{command}</text>
            {year}
            <rect x="160" y="740" width={underline.max(0.5)} height="10" fill={ORANGE} />
            <g opacity={meta}>
                <text x="160" y="826" font-family={MONO} font-weight="600" font-size="36" fill={ORANGE}>{first.hash}</text>
                <text x={160.0 + hash_w + 30.0} y="826" font-family={MONO} font-weight="500" font-size="36" fill={BONE}>{date_and_author}</text>
                <text x="1030" y="832" font-family={SERIF} font-style="italic" font-size="64" fill={ORANGE}>{message}</text>
            </g>
            {callout(160.0 + hash_w / 2.0, 842.0, 160.0 + hash_w / 2.0, 980.0, "FIRST COMMIT".to_owned(), ORANGE, prog(lb, 5.0, 5.8))}
            {callout(1270.0, 520.0, 1420.0, 360.0, years_ago, BONE, prog(lb, 5.5, 6.3))}
            {title_block(first.date, prog(lb, 6.0, 6.4))}
        </g>
    )
}

/// Engineering-drawing title block, bottom right.
fn title_block(date: &'static str, o: f32) -> Svgr<'static> {
    let sheets = format!("1 OF {}", HISTORY.count);
    if o <= 0.0 {
        return Svgr::empty();
    }
    fframes::svgr!(
        <g opacity={o} font-family={MONO} font-size="18" letter-spacing="1.5" fill={BONE}>
            <rect x="1290" y="800" width="470" height="150" fill="#0b0b0b" fill-opacity="0.7" stroke={GREY} stroke-width="1.5" />
            <path d="M1290 850 H1760 M1290 900 H1760 M1400 800 V950 M1600 850 V950" stroke={GREY} stroke-width="1.5" />
            <text x="1306" y="832" fill={GREY}>"TITLE"</text>
            <text x="1416" y="832">"FFRAMES — PROOF OF CONCEPT"</text>
            <text x="1306" y="882" fill={GREY}>"DRAWN"</text>
            <text x="1416" y="882">"dmtrKovalenko"</text>
            <text x="1616" y="882" fill={GREY}>"REV 1"</text>
            <text x="1306" y="932" fill={GREY}>"SHEET"</text>
            <text x="1416" y="932">{sheets}</text>
            <text x="1616" y="932" fill={ORANGE}>{date}</text>
        </g>
    )
}

/// Beats 8-16: every commit scrolls past, accelerating, the year flips.
fn history(lb: f32) -> Svgr<'static> {
    let commits = &*COMMITS;
    let n = commits.len();
    if n == 0 {
        return Svgr::empty();
    }
    let enter = expo_out(prog(lb, 0.0, 0.5));
    let exit = expo_in(prog(lb, 7.6, 8.0));
    // position in the list (float), ease in-out across the 8 beats
    let pos = cubic_in_out(prog(lb, 0.3, 7.4)) * (n - 1) as f32;
    let current = pos.round() as usize;
    let line_h = 44.0;
    let center_y = 560.0;
    let first = pos.floor() as i32 - 11;
    let rows: Vec<Svgr> = (first..first + 24)
        .filter(|i| *i >= 0 && (*i as usize) < n)
        .map(|i| {
            let c = &commits[i as usize];
            let y = center_y + (i as f32 - pos) * line_h;
            let dist = ((y - center_y) / 480.0).abs();
            let o = (1.0 - dist).clamp(0.0, 1.0).powf(1.5);
            let is_current = i as usize == current;
            let msg: String = c.message.chars().filter(|ch| ch.is_ascii()).take(48).collect();
            let text_color = if is_current { BONE } else { "#8f8a82" };
            fframes::svgr!(
                <g opacity={o}>
                    <text x="160" y={y} font-family={MONO} font-weight="600" font-size="26" fill={if is_current { ORANGE } else { "#6d4a36" }}>{c.hash}</text>
                    <text x="310" y={y} font-family={MONO} font-weight="500" font-size="26" fill={GREY}>{c.date}</text>
                    <text x="510" y={y} font-family={MONO} font-weight="500" font-size="26" fill={text_color}>{msg}</text>
                </g>
            )
        })
        .collect();
    let c = &commits[current.min(n - 1)];
    let year = c.date[..4].to_owned();
    let count = format!("#{:03} / {}", current + 1, n);
    let date = c.date.to_owned();

    fframes::svgr!(
        <g opacity={enter * (1.0 - exit)} transform={format!("translate(0 {})", (1.0 - enter) * 80.0 - exit * 80.0)}>
            <rect x="140" y={center_y - 34.0} width="1060" height="48" fill={ORANGE} fill-opacity="0.12" />
            <rect x="140" y={center_y - 34.0} width="4" height="48" fill={ORANGE} />
            {rows}
            <text x="1770" y="600" text-anchor="end" font-family={DISPLAY} font-size="300" letter-spacing="-10" fill={ORANGE}>{year}</text>
            <text x="1770" y="680" text-anchor="end" font-family={MONO} font-weight="500" font-size="36" fill={BONE}>{count}</text>
            <text x="1770" y="730" text-anchor="end" font-family={MONO} font-weight="500" font-size="26" fill={GREY}>{date}</text>
        </g>
    )
}

/// Beats 16-32: the numbers, one per bar, then "video = f(frame)".
fn stats(frame: &mut Frame, ctx: &FFramesContext, lb: f32) -> Svgr<'static> {
    let h = &*HISTORY;
    // (value, unit, caption); values count up as they enter
    let items: [(u64, &str, String); 3] = [
        (
            (h.days / 365) as u64,
            "YEARS",
            format!(
                "{} → {}  ·  {} DAYS",
                h.since,
                h.until,
                thousands(h.days as u64)
            ),
        ),
        (
            h.count as u64,
            "COMMITS",
            "FROM THE FIRST “POC” TO TODAY".to_owned(),
        ),
        (
            crate::facts::LINES_OF_RUST,
            "LINES OF RUST",
            "FFMPEG · SVG · SKIA · EDITOR · CLI".to_owned(),
        ),
    ];
    let bar = (lb / 4.0).floor() as usize;
    let local = lb - bar as f32 * 4.0;
    if bar < 3 {
        let (value, unit, caption) = items[bar].clone();
        let num = thousands(value);
        let enter = expo_out(prog(local, 0.0, 0.7));
        let exit = expo_in(prog(local, 3.65, 4.0));
        let x = 150.0 + (1.0 - enter) * 260.0 - exit * 300.0;
        let num_w = measure(frame, ctx, DISPLAY, 400, 400, false, &num) - 10.0 * num.len() as f32;
        let unit_s = snap(local - 0.5);
        let unit_w = measure(frame, ctx, DISPLAY, 110, 400, false, unit) - 3.0 * unit.len() as f32;
        // the unit goes next to the number when it fits, below it otherwise
        let (ux, uy, cap_y) = if 150.0 + num_w + 40.0 + unit_w < 1770.0 {
            (num_w + 40.0, 0.0, 0.0)
        } else {
            (8.0, 130.0, 130.0)
        };
        let cap = prog(local, 1.0, 1.4);
        let shown = thousands((value as f32 * expo_out(prog(local, 0.0, 1.3))).round() as u64);
        return fframes::svgr!(
            <g opacity={1.0 - exit}>
                <g transform={format!("translate({x} 0)")}>
                    <text x="0" y="640" font-family={DISPLAY} font-size="400" letter-spacing="-10" fill={BONE}>{shown}</text>
                    <g transform={format!("translate({} {})", ux, uy + (1.0 - unit_s) * 60.0)} opacity={unit_s.min(1.0)}>
                        <text x="0" y="640" font-family={DISPLAY} font-size="110" letter-spacing="-3" fill={ORANGE}>{unit}</text>
                    </g>
                    <rect x="4" y={700.0 + cap_y} width={(cap * 60.0).max(0.5)} height="4" fill={ORANGE} />
                    <text x="4" y={760.0 + cap_y} font-family={MONO} font-weight="500" font-size="30" letter-spacing="3" fill={BONE} opacity={cap}>{caption}</text>
                </g>
            </g>
        );
    }
    // bar 4: ONE IDEA, then video = f(frame)
    let a = snap(local);

    let exit = expo_in(prog(local, 3.7, 4.0));
    fframes::svgr!(
        <g opacity={1.0 - exit}>
            <text x="160" y="330" font-family={MONO} font-weight="600" font-size="34" letter-spacing="6" fill={ORANGE} opacity={a.min(1.0)}>"ONE IDEA"</text>
            {Slam::new(150.0, 620.0, "video =", DISPLAY, 190.0, BONE).draw(local - 1.0, 0.0, 150.0)}
            {Slam::new(905.0, 628.0, "f(frame)", SERIF, 270.0, ORANGE).italic().draw(local - 2.0, 260.0, 0.0)}
        </g>
    )
}
