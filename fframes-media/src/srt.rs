//! `SubRip` (`.srt`) cue parsing for [`CaptionTrack::from_srt`](crate::CaptionTrack::from_srt).
//!
//! A cue is an optional number line, a `00:00:01,000 --> 00:00:02,500` timing line and one or
//! more text lines, ended by a blank line. Text is kept raw, like `WebVTT` payloads: inline
//! tags such as `<i>` are left for a renderer to interpret.

use crate::CaptionError;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SrtCue {
    /// The cue number as written. `SubRip` numbers are not guaranteed to be unique.
    pub number: Option<String>,
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// Whether a document without a `WEBVTT` header or JSON object looks like `SubRip`:
/// its first line is a cue number or a timing line.
pub(crate) fn looks_like_srt(content: &str) -> bool {
    lines(content)
        .map(|(_, line)| line.trim())
        .find(|line| !line.is_empty())
        .is_some_and(|line| is_cue_number(line) || line.contains("-->"))
}

/// Parse every cue in document order. Errors name the 1-based line they occur on.
/// Cues without text are dropped, as `WebVTT` spacer cues are.
pub(crate) fn parse(content: &str) -> Result<Vec<SrtCue>, CaptionError> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut lines = lines(content).peekable();
    let mut cues = Vec::new();
    while let Some((line_number, line)) = lines.next() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (number, timing_line_number, timing) = if line.contains("-->") {
            (None, line_number, line)
        } else if is_cue_number(line) {
            match lines.next() {
                Some((next_number, next)) if next.contains("-->") => {
                    (Some(line.to_owned()), next_number, next.trim())
                }
                _ => {
                    return Err(error(
                        line_number + 1,
                        "expected a timing line such as `00:00:01,000 --> 00:00:02,000` after the cue number",
                    ));
                }
            }
        } else {
            return Err(error(
                line_number,
                format!("expected a cue number or a timing line, found `{line}`"),
            ));
        };
        let (start, end) = parse_timing(timing, timing_line_number)?;

        let mut text = String::new();
        while let Some((_, text_line)) = lines.next_if(|(_, line)| !line.trim().is_empty()) {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(text_line.trim_end());
        }
        if !text.is_empty() {
            cues.push(SrtCue {
                number,
                start,
                end,
                text,
            });
        }
    }
    Ok(cues)
}

/// Split on `\n`, `\r\n` or a lone `\r`, numbering lines from 1.
fn lines(content: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut rest = Some(content);
    std::iter::from_fn(move || {
        let current = rest?;
        if let Some(index) = current.find(['\r', '\n']) {
            let skip = if current[index..].starts_with("\r\n") {
                2
            } else {
                1
            };
            rest = Some(&current[index + skip..]);
            Some(&current[..index])
        } else {
            rest = None;
            Some(current)
        }
    })
    .enumerate()
    .map(|(index, line)| (index + 1, line))
}

fn is_cue_number(line: &str) -> bool {
    !line.is_empty() && line.bytes().all(|byte| byte.is_ascii_digit())
}

fn parse_timing(line: &str, line_number: usize) -> Result<(f64, f64), CaptionError> {
    let (start, rest) = line
        .split_once("-->")
        .expect("timing lines contain an arrow");
    // Anything after the end time is legacy SubRip positioning (`X1:… Y1:…`), ignored here.
    let end = rest.split_whitespace().next().unwrap_or_default();
    let start = parse_timestamp(start.trim(), line_number)?;
    let end = parse_timestamp(end, line_number)?;
    if end < start {
        return Err(error(line_number, "cue ends before it starts"));
    }
    Ok((start, end))
}

/// `HH:MM:SS,mmm`. Also accepts `.` before the fraction, one to three fraction digits,
/// any number of hour digits, and `MM:SS,mmm` without hours.
fn parse_timestamp(value: &str, line_number: usize) -> Result<f64, CaptionError> {
    let invalid = || {
        error(
            line_number,
            format!("invalid timestamp `{value}`, expected `HH:MM:SS,mmm`"),
        )
    };
    let (clock, fraction) = value.split_once([',', '.']).ok_or_else(invalid)?;
    if fraction.is_empty()
        || fraction.len() > 3
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(invalid());
    }
    let fraction_milliseconds =
        fraction.parse::<u64>().map_err(|_| invalid())? * 10u64.pow(3 - fraction.len() as u32);

    let parts = clock.split(':').collect::<Vec<_>>();
    let (hours, minutes, seconds) = match parts.as_slice() {
        [hours, minutes, seconds] => (*hours, *minutes, *seconds),
        [minutes, seconds] => ("0", *minutes, *seconds),
        _ => return Err(invalid()),
    };
    let number = |part: &str, two_digits: bool| -> Result<u64, CaptionError> {
        if part.is_empty()
            || (two_digits && part.len() != 2)
            || !part.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(invalid());
        }
        part.parse().map_err(|_| invalid())
    };
    let hours = number(hours, false)?;
    let minutes = number(minutes, true)?;
    let seconds = number(seconds, true)?;
    if minutes >= 60 || seconds >= 60 {
        return Err(invalid());
    }
    let milliseconds = hours
        .checked_mul(3_600_000)
        .and_then(|value| {
            value.checked_add((minutes * 60 + seconds) * 1000 + fraction_milliseconds)
        })
        .ok_or_else(invalid)?;
    Ok(milliseconds as f64 / 1000.0)
}

fn error(line: usize, message: impl Into<String>) -> CaptionError {
    CaptionError::invalid(format!("SRT line {line}"), message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(number: Option<&str>, start: f64, end: f64, text: &str) -> SrtCue {
        SrtCue {
            number: number.map(str::to_owned),
            start,
            end,
            text: text.to_owned(),
        }
    }

    fn error_message(content: &str) -> String {
        parse(content).unwrap_err().to_string()
    }

    #[test]
    fn parses_numbered_cues_and_multiline_text() {
        let cues = parse(
            "1\n00:00:01,000 --> 00:00:02,500\nFirst line\nSecond line\n\n\
             2\n00:00:03,000 --> 00:00:04,000\nNext\n",
        )
        .unwrap();
        assert_eq!(
            cues,
            [
                cue(Some("1"), 1.0, 2.5, "First line\nSecond line"),
                cue(Some("2"), 3.0, 4.0, "Next"),
            ]
        );
    }

    #[test]
    fn handles_bom_crlf_lone_cr_and_trailing_whitespace() {
        let expected = [
            cue(Some("1"), 0.5, 1.0, "Hello\nworld"),
            cue(Some("2"), 2.0, 3.0, "Again"),
        ];
        for document in [
            "\u{feff}1\r\n00:00:00,500 --> 00:00:01,000\r\nHello  \r\nworld\r\n\r\n2\r\n00:00:02,000 --> 00:00:03,000\r\nAgain\r\n",
            "1\r00:00:00,500 --> 00:00:01,000\rHello\rworld\r\r2\r00:00:02,000 --> 00:00:03,000\rAgain",
            "\n\n1\n00:00:00,500 --> 00:00:01,000\nHello\nworld\n \t\n\n\n2\n00:00:02,000 --> 00:00:03,000\nAgain\n\n\n",
        ] {
            assert_eq!(parse(document).unwrap(), expected, "{document:?}");
        }
    }

    #[test]
    fn keeps_overlapping_and_out_of_order_cues_in_document_order() {
        let cues = parse(
            "1\n00:00:01,000 --> 00:00:05,000\nNarrator\n\n\
             2\n00:00:02,000 --> 00:00:03,000\nOverlaps\n\n\
             3\n00:00:00,000 --> 00:00:00,800\nEarlier\n",
        )
        .unwrap();
        assert_eq!(
            cues.iter().map(|cue| cue.start).collect::<Vec<_>>(),
            [1.0, 2.0, 0.0]
        );
    }

    #[test]
    fn accepts_common_timestamp_variants() {
        let cues = parse(
            "00:00:01.250 --> 00:00:02,5 X1:10 X2:20 Y1:30 Y2:40\nNo number, dot, short fraction\n\n\
             7\n01:02:03,004 --> 101:00:00,000\nLong hours\n\n\
             8\n00:05,000 --> 00:06,000\nNo hours\n",
        )
        .unwrap();
        assert_eq!(
            cues[0],
            cue(None, 1.25, 2.5, "No number, dot, short fraction")
        );
        assert_eq!(cues[1].start, 3723.004);
        assert_eq!(cues[1].end, 363_600.0);
        assert_eq!((cues[2].start, cues[2].end), (5.0, 6.0));
    }

    #[test]
    fn keeps_raw_markup_and_numeric_text_lines() {
        let cues = parse("1\n00:00:01,000 --> 00:00:02,000\n<i>The year</i>\n1984\n").unwrap();
        assert_eq!(cues, [cue(Some("1"), 1.0, 2.0, "<i>The year</i>\n1984")]);
    }

    #[test]
    fn drops_cues_without_text_and_allows_instant_cues() {
        let cues = parse(
            "1\n00:00:01,000 --> 00:00:02,000\n\n\
             2\n00:00:03,000 --> 00:00:03,000\nInstant\n",
        )
        .unwrap();
        assert_eq!(cues, [cue(Some("2"), 3.0, 3.0, "Instant")]);
        assert_eq!(parse("").unwrap(), []);
        assert_eq!(parse("\u{feff}\r\n\r\n").unwrap(), []);
    }

    #[test]
    fn rejects_malformed_lines_with_their_line_number() {
        for (document, expected) in [
            (
                "Hello\n",
                "SRT line 1: expected a cue number or a timing line",
            ),
            ("1\nHello\n", "SRT line 2: expected a timing line"),
            ("1\n", "SRT line 2: expected a timing line"),
            (
                "1\n\n00:00:01,000 --> 00:00:02,000\nx\n",
                "SRT line 2: expected a timing line",
            ),
            (
                "1\n00:00:01 --> 00:00:02,000\nx\n",
                "SRT line 2: invalid timestamp `00:00:01`",
            ),
            (
                "1\n00:00:01,000 -->\nx\n",
                "SRT line 2: invalid timestamp ``",
            ),
            (
                "1\n00:00:01,0000 --> 00:00:02,000\nx\n",
                "invalid timestamp",
            ),
            (
                "1\n00:61:01,000 --> 00:62:02,000\nx\n",
                "invalid timestamp `00:61:01,000`",
            ),
            ("1\n00:00:1,000 --> 00:00:02,000\nx\n", "invalid timestamp"),
            (
                "1\n-00:00:01,000 --> 00:00:02,000\nx\n",
                "invalid timestamp",
            ),
            ("1\n0:0:0:1,000 --> 00:00:02,000\nx\n", "invalid timestamp"),
            (
                "1\n00:00:05,000 --> 00:00:02,000\nx\n",
                "SRT line 2: cue ends before it starts",
            ),
            (
                "1\n00:00:01,000 --> 00:00:02,000\nOne\n\nstray text after a blank line\n",
                "SRT line 5: expected a cue number or a timing line, found `stray text after a blank line`",
            ),
        ] {
            let message = error_message(document);
            assert!(message.contains(expected), "{document:?}: {message}");
        }
    }

    #[test]
    fn detects_srt_by_its_first_line() {
        assert!(looks_like_srt("\n\n1\n00:00:01,000 --> 00:00:02,000\nx"));
        assert!(looks_like_srt("00:00:01,000 --> 00:00:02,000\nx"));
        assert!(!looks_like_srt("Hello\n00:00:01,000 --> 00:00:02,000"));
        assert!(!looks_like_srt(""));
    }
}
