//! Subtitle format parser — SRT, ASS/SSA, `WebVTT`.
//!
//! Provides pure-Rust parsing for the three most common text subtitle formats.
//! Malformed events are skipped with a `log::warn!`; a file with zero valid
//! events returns [`SubtitleError::NoEvents`].
//!
//! # Example
//!
//! ```
//! use ff_format::subtitle::{SubtitleTrack, SubtitleError};
//!
//! let srt = "1\n00:00:01,000 --> 00:00:04,000\nHello world\n\n";
//! let track = SubtitleTrack::from_srt(srt).unwrap();
//! assert_eq!(track.events.len(), 1);
//! assert_eq!(track.events[0].text, "Hello world");
//! ```

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use thiserror::Error;

/// Error type for subtitle parsing operations.
#[derive(Debug, Error)]
pub enum SubtitleError {
    /// I/O error reading a subtitle file.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// File extension is not a recognized subtitle format.
    #[error("unsupported subtitle format: {extension}")]
    UnsupportedFormat {
        /// The unrecognized file extension.
        extension: String,
    },

    /// A structural parse error prevents processing the file.
    #[error("parse error at line {line}: {reason}")]
    ParseError {
        /// 1-based line number where the error was detected.
        line: usize,
        /// Human-readable description of the problem.
        reason: String,
    },

    /// The input contained no valid subtitle events.
    #[error("no valid subtitle events found")]
    NoEvents,
}

/// A single subtitle event (cue).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtitleEvent {
    /// Sequential 0-based event index.
    pub index: usize,
    /// Presentation start time.
    pub start: Duration,
    /// Presentation end time.
    pub end: Duration,
    /// Plain text with all style/override tags stripped.
    pub text: String,
    /// Original text including any style or override tags.
    pub raw: String,
    /// Additional metadata fields (e.g. ASS `Actor`, `Style`).
    pub metadata: HashMap<String, String>,
}

/// A parsed subtitle track containing ordered events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtitleTrack {
    /// Ordered list of subtitle events.
    pub events: Vec<SubtitleEvent>,
    /// BCP-47 language tag when available (e.g. `"en"`, `"ja"`).
    pub language: Option<String>,
}

impl SubtitleTrack {
    /// Parse a `SubRip` (`.srt`) subtitle string.
    ///
    /// Supports multi-line cues and HTML-style tags (`<i>`, `<b>`, `<u>`).
    /// Malformed blocks are skipped with `log::warn!`.
    ///
    /// # Errors
    ///
    /// Returns [`SubtitleError::NoEvents`] when no valid events are found.
    pub fn from_srt(input: &str) -> Result<Self, SubtitleError> {
        parse_srt(input)
    }

    /// Parse an ASS/SSA subtitle string.
    ///
    /// Reads the `[Events]` section only. Override tags (`{...}`) are
    /// preserved in [`SubtitleEvent::raw`] and stripped for
    /// [`SubtitleEvent::text`]. Malformed `Dialogue:` lines are skipped.
    ///
    /// # Errors
    ///
    /// Returns [`SubtitleError::NoEvents`] when no valid events are found.
    pub fn from_ass(input: &str) -> Result<Self, SubtitleError> {
        parse_ass(input)
    }

    /// Parse a `WebVTT` (`.vtt`) subtitle string.
    ///
    /// Cue identifiers are optional. Voice span tags (`<v Speaker>`) and
    /// other HTML tags are stripped for [`SubtitleEvent::text`]. Malformed
    /// cues are skipped with `log::warn!`.
    ///
    /// # Errors
    ///
    /// Returns [`SubtitleError::ParseError`] when the `WEBVTT` header is
    /// missing, or [`SubtitleError::NoEvents`] when no valid cues are found.
    pub fn from_vtt(input: &str) -> Result<Self, SubtitleError> {
        parse_vtt(input)
    }

    /// Load and parse a subtitle file, auto-detecting the format by extension.
    ///
    /// Supported extensions: `.srt`, `.ass`, `.ssa`, `.vtt`.
    ///
    /// # Errors
    ///
    /// Returns [`SubtitleError::UnsupportedFormat`] for unrecognized extensions,
    /// [`SubtitleError::Io`] on read failure, or a format-specific error when
    /// parsing fails.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, SubtitleError> {
        let path = path.as_ref();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        // Validate extension before performing I/O.
        match ext.as_str() {
            "srt" | "ass" | "ssa" | "vtt" => {}
            _ => return Err(SubtitleError::UnsupportedFormat { extension: ext }),
        }

        let content = std::fs::read_to_string(path)?;

        match ext.as_str() {
            "srt" => parse_srt(&content),
            "ass" | "ssa" => parse_ass(&content),
            "vtt" => parse_vtt(&content),
            _ => unreachable!("extension validated above"),
        }
    }
}

// ── SRT parser ────────────────────────────────────────────────────────────────

fn parse_srt(input: &str) -> Result<SubtitleTrack, SubtitleError> {
    let mut events: Vec<SubtitleEvent> = Vec::new();
    let mut current_block: Vec<String> = Vec::new();

    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !current_block.is_empty() {
                if let Some(ev) = parse_srt_block(&current_block, events.len()) {
                    events.push(ev);
                }
                current_block.clear();
            }
        } else {
            current_block.push(trimmed.to_string());
        }
    }

    // Handle last block without a trailing blank line.
    if !current_block.is_empty()
        && let Some(ev) = parse_srt_block(&current_block, events.len())
    {
        events.push(ev);
    }

    if events.is_empty() {
        return Err(SubtitleError::NoEvents);
    }

    Ok(SubtitleTrack {
        events,
        language: None,
    })
}

fn parse_srt_block(block: &[String], index: usize) -> Option<SubtitleEvent> {
    if block.len() < 3 {
        log::warn!(
            "srt block has too few lines, skipping count={}",
            block.len()
        );
        return None;
    }

    // First line: 1-based sequence number.
    if block[0].parse::<usize>().is_err() {
        log::warn!(
            "srt block index is not a number, skipping value={}",
            block[0]
        );
        return None;
    }

    let Some((start, end)) = parse_srt_timestamp_line(&block[1]) else {
        log::warn!("srt malformed timestamp line, skipping line={}", block[1]);
        return None;
    };

    let raw = block[2..].join("\n");
    let text = strip_html_tags(&raw);

    Some(SubtitleEvent {
        index,
        start,
        end,
        text,
        raw,
        metadata: HashMap::new(),
    })
}

fn parse_srt_timestamp_line(line: &str) -> Option<(Duration, Duration)> {
    let mut parts = line.splitn(2, " --> ");
    let start = parse_srt_timestamp(parts.next()?.trim())?;
    let end = parse_srt_timestamp(parts.next()?.trim())?;
    Some((start, end))
}

/// Parse `HH:MM:SS,mmm` (comma or period separator) into a [`Duration`].
fn parse_srt_timestamp(s: &str) -> Option<Duration> {
    let s = s.replace(',', ".");
    let (hms_str, ms_str) = match s.split_once('.') {
        Some((h, m)) => (h, m),
        None => (s.as_str(), "0"),
    };
    let ms: u64 = ms_str.parse().ok()?;
    let hms: Vec<u64> = hms_str
        .split(':')
        .map(|p| p.parse().ok())
        .collect::<Option<Vec<_>>>()?;
    if hms.len() != 3 {
        return None;
    }
    let total_ms = hms[0] * 3_600_000 + hms[1] * 60_000 + hms[2] * 1_000 + ms;
    Some(Duration::from_millis(total_ms))
}

// ── ASS/SSA parser ─────────────────────────────────────────────────────────────

fn parse_ass(input: &str) -> Result<SubtitleTrack, SubtitleError> {
    let mut events: Vec<SubtitleEvent> = Vec::new();
    let mut in_events = false;
    let mut format_cols: Vec<String> = Vec::new();

    for (line_no, line) in input.lines().enumerate() {
        let line = line.trim();

        if line.eq_ignore_ascii_case("[Events]") {
            in_events = true;
            continue;
        }

        // New section header ends the [Events] block.
        if line.starts_with('[') && in_events {
            break;
        }

        if !in_events {
            continue;
        }

        if let Some(rest) = line.strip_prefix("Format:") {
            format_cols = rest.split(',').map(|c| c.trim().to_string()).collect();
            continue;
        }

        let Some(rest) = line.strip_prefix("Dialogue:") else {
            continue;
        };

        if format_cols.is_empty() {
            log::warn!(
                "ass dialogue line found before Format line at line={}",
                line_no + 1
            );
            continue;
        }

        let num_cols = format_cols.len();
        let parts: Vec<&str> = rest.splitn(num_cols, ',').collect();
        if parts.len() < num_cols {
            log::warn!(
                "ass dialogue has fewer fields than format at line={}",
                line_no + 1
            );
            continue;
        }

        let col_map: HashMap<&str, &str> = format_cols
            .iter()
            .zip(parts.iter())
            .map(|(k, v)| (k.as_str(), v.trim()))
            .collect();

        let Some(start) = col_map.get("Start").and_then(|s| parse_ass_timestamp(s)) else {
            log::warn!("ass malformed start timestamp at line={}", line_no + 1);
            continue;
        };

        let Some(end) = col_map.get("End").and_then(|s| parse_ass_timestamp(s)) else {
            log::warn!("ass malformed end timestamp at line={}", line_no + 1);
            continue;
        };

        let raw = col_map.get("Text").copied().unwrap_or("").to_string();
        let text = strip_ass_tags(&raw);

        let mut metadata = HashMap::new();
        for key in &["Style", "Name", "Actor", "Layer", "Effect"] {
            if let Some(val) = col_map.get(key)
                && !val.is_empty()
            {
                metadata.insert((*key).to_string(), (*val).to_string());
            }
        }

        events.push(SubtitleEvent {
            index: events.len(),
            start,
            end,
            text,
            raw,
            metadata,
        });
    }

    if events.is_empty() {
        return Err(SubtitleError::NoEvents);
    }

    Ok(SubtitleTrack {
        events,
        language: None,
    })
}

/// Parse `H:MM:SS.cc` (centiseconds) into a [`Duration`].
fn parse_ass_timestamp(s: &str) -> Option<Duration> {
    let (hms_str, cs_str) = match s.split_once('.') {
        Some((h, c)) => (h, c),
        None => (s, "0"),
    };
    let cs: u64 = cs_str.parse().ok()?;
    let hms: Vec<u64> = hms_str
        .split(':')
        .map(|p| p.parse().ok())
        .collect::<Option<Vec<_>>>()?;
    if hms.len() != 3 {
        return None;
    }
    let total_ms = hms[0] * 3_600_000 + hms[1] * 60_000 + hms[2] * 1_000 + cs * 10;
    Some(Duration::from_millis(total_ms))
}

// ── WebVTT parser ──────────────────────────────────────────────────────────────

fn parse_vtt(input: &str) -> Result<SubtitleTrack, SubtitleError> {
    let mut lines_iter = input.lines();

    // The first line must start with "WEBVTT".
    match lines_iter.next() {
        Some(first) if first.trim_start_matches('\u{FEFF}').starts_with("WEBVTT") => {}
        _ => {
            return Err(SubtitleError::ParseError {
                line: 1,
                reason: "WebVTT file must begin with WEBVTT".to_string(),
            });
        }
    }

    let mut events: Vec<SubtitleEvent> = Vec::new();
    let mut current_block: Vec<String> = Vec::new();

    for line in lines_iter {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !current_block.is_empty() {
                if let Some(ev) = parse_vtt_block(&current_block, events.len()) {
                    events.push(ev);
                }
                current_block.clear();
            }
        } else {
            current_block.push(trimmed.to_string());
        }
    }

    // Handle last block without a trailing blank line.
    if !current_block.is_empty()
        && let Some(ev) = parse_vtt_block(&current_block, events.len())
    {
        events.push(ev);
    }

    if events.is_empty() {
        return Err(SubtitleError::NoEvents);
    }

    Ok(SubtitleTrack {
        events,
        language: None,
    })
}

fn parse_vtt_block(block: &[String], index: usize) -> Option<SubtitleEvent> {
    // Skip metadata blocks.
    let first = block[0].as_str();
    if first.starts_with("NOTE") || first.starts_with("STYLE") || first.starts_with("REGION") {
        return None;
    }

    // Find the line containing "-->".
    let Some(ts_idx) = block.iter().position(|l| l.contains("-->")) else {
        log::warn!("vtt block has no timestamp line, skipping block_start={first}");
        return None;
    };

    let Some((start, end)) = parse_vtt_timestamp_line(&block[ts_idx]) else {
        log::warn!(
            "vtt malformed timestamp line, skipping line={}",
            block[ts_idx]
        );
        return None;
    };

    if ts_idx + 1 >= block.len() {
        log::warn!("vtt cue has no text start={start:?}");
        return None;
    }

    let raw = block[ts_idx + 1..].join("\n");
    let text = strip_html_tags(&raw);

    Some(SubtitleEvent {
        index,
        start,
        end,
        text,
        raw,
        metadata: HashMap::new(),
    })
}

fn parse_vtt_timestamp_line(line: &str) -> Option<(Duration, Duration)> {
    let mut parts = line.splitn(2, " --> ");
    let start = parse_vtt_timestamp(parts.next()?.trim())?;
    // End timestamp may be followed by cue settings (e.g. `align:center`).
    let end_part = parts.next()?.trim();
    let end_str = end_part.split_whitespace().next().unwrap_or("");
    let end = parse_vtt_timestamp(end_str)?;
    Some((start, end))
}

/// Parse `HH:MM:SS.mmm` or `MM:SS.mmm` into a [`Duration`].
fn parse_vtt_timestamp(s: &str) -> Option<Duration> {
    let (hms_str, ms_str) = match s.split_once('.') {
        Some((h, m)) => (h, m),
        None => (s, "0"),
    };
    // Normalise to exactly 3 digits for milliseconds.
    let ms_padded = format!("{ms_str:0<3}");
    let ms: u64 = ms_padded[..3.min(ms_padded.len())].parse().ok()?;
    let hms: Vec<u64> = hms_str
        .split(':')
        .map(|p| p.parse().ok())
        .collect::<Option<Vec<_>>>()?;
    let total_ms = match hms.len() {
        2 => hms[0] * 60_000 + hms[1] * 1_000 + ms,
        3 => hms[0] * 3_600_000 + hms[1] * 60_000 + hms[2] * 1_000 + ms,
        _ => return None,
    };
    Some(Duration::from_millis(total_ms))
}

// ── Tag stripping helpers ──────────────────────────────────────────────────────

/// Strip HTML-style tags (`<tag>`, `</tag>`) from `s`.
fn strip_html_tags(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(c),
            _ => {}
        }
    }
    result
}

/// Strip ASS override tags (`{...}`) and convert soft line-breaks (`\N`, `\n`).
fn strip_ass_tags(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_tag = false;
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '{' => {
                in_tag = true;
                i += 1;
            }
            '}' => {
                in_tag = false;
                i += 1;
            }
            '\\' if !in_tag && i + 1 < chars.len() => match chars[i + 1] {
                'N' | 'n' => {
                    result.push('\n');
                    i += 2;
                }
                _ => {
                    result.push(chars[i]);
                    i += 1;
                }
            },
            c if !in_tag => {
                result.push(c);
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }
    result
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // ── SRT ───────────────────────────────────────────────────────────────────

    #[test]
    fn from_srt_should_parse_single_event() {
        let input = "1\n00:00:01,000 --> 00:00:04,000\nHello world\n";
        let track = SubtitleTrack::from_srt(input).unwrap();
        assert_eq!(track.events.len(), 1);
        let ev = &track.events[0];
        assert_eq!(ev.index, 0);
        assert_eq!(ev.start, Duration::from_millis(1_000));
        assert_eq!(ev.end, Duration::from_millis(4_000));
        assert_eq!(ev.text, "Hello world");
        assert_eq!(ev.raw, "Hello world");
    }

    #[test]
    fn from_srt_should_parse_multiline_text() {
        let input = "1\n00:00:01,000 --> 00:00:04,000\nLine one\nLine two\n\n2\n00:00:05,000 --> 00:00:07,000\nSecond\n";
        let track = SubtitleTrack::from_srt(input).unwrap();
        assert_eq!(track.events.len(), 2);
        assert_eq!(track.events[0].text, "Line one\nLine two");
        assert_eq!(track.events[1].text, "Second");
    }

    #[test]
    fn from_srt_should_strip_html_tags_preserving_raw() {
        let input = "1\n00:00:01,000 --> 00:00:04,000\n<i>Italic</i> and <b>bold</b>\n";
        let track = SubtitleTrack::from_srt(input).unwrap();
        let ev = &track.events[0];
        assert_eq!(ev.text, "Italic and bold");
        assert_eq!(ev.raw, "<i>Italic</i> and <b>bold</b>");
    }

    #[test]
    fn from_srt_should_skip_malformed_event_and_parse_rest() {
        let input = "1\n00:00:01,000 --> 00:00:04,000\nGood\n\nNOT_NUM\nbad ts\ntext\n\n2\n00:00:05,000 --> 00:00:07,000\nAlso good\n";
        let track = SubtitleTrack::from_srt(input).unwrap();
        assert_eq!(track.events.len(), 2);
        assert_eq!(track.events[0].text, "Good");
        assert_eq!(track.events[1].text, "Also good");
    }

    #[test]
    fn from_srt_should_return_no_events_for_empty_input() {
        let result = SubtitleTrack::from_srt("");
        assert!(matches!(result, Err(SubtitleError::NoEvents)));
    }

    #[test]
    fn from_srt_should_return_no_events_when_all_blocks_malformed() {
        let result = SubtitleTrack::from_srt("NOT_NUM\n00:00:01,000 --> 00:00:04,000\ntext\n");
        assert!(matches!(result, Err(SubtitleError::NoEvents)));
    }

    // ── ASS ───────────────────────────────────────────────────────────────────

    const ASS_SAMPLE: &str = "\
[Script Info]
Title: Test

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Dialogue: 0,0:00:01.00,0:00:04.00,Default,,0,0,0,,Hello {\\i1}world{\\i0}
Dialogue: 0,0:00:05.00,0:00:07.00,Default,,0,0,0,,Second line
";

    #[test]
    fn from_ass_should_parse_dialogue_events() {
        let track = SubtitleTrack::from_ass(ASS_SAMPLE).unwrap();
        assert_eq!(track.events.len(), 2);
        let ev = &track.events[0];
        assert_eq!(ev.start, Duration::from_millis(1_000));
        assert_eq!(ev.end, Duration::from_millis(4_000));
        assert!(ev.raw.contains("{\\i1}"));
        assert!(!ev.text.contains('{'));
    }

    #[test]
    fn from_ass_should_strip_override_tags_preserving_raw() {
        let input = "[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:04.00,Default,,0,0,0,,{\\pos(100,200)}Hello\n";
        let track = SubtitleTrack::from_ass(input).unwrap();
        let ev = &track.events[0];
        assert_eq!(ev.text, "Hello");
        assert!(ev.raw.contains("{\\pos"));
    }

    #[test]
    fn from_ass_should_populate_metadata_fields() {
        let input = "[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:04.00,Signs,Actor1,0,0,0,,text\n";
        let track = SubtitleTrack::from_ass(input).unwrap();
        let ev = &track.events[0];
        assert_eq!(ev.metadata.get("Style"), Some(&"Signs".to_string()));
        assert_eq!(ev.metadata.get("Name"), Some(&"Actor1".to_string()));
    }

    #[test]
    fn from_ass_should_return_no_events_for_empty_events_section() {
        let input = "[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n";
        let result = SubtitleTrack::from_ass(input);
        assert!(matches!(result, Err(SubtitleError::NoEvents)));
    }

    // ── VTT ───────────────────────────────────────────────────────────────────

    const VTT_SAMPLE: &str = "\
WEBVTT

1
00:00:01.000 --> 00:00:04.000
Hello world

00:00:05.000 --> 00:00:07.000 align:center
<v Speaker>Voice tagged text</v>
";

    #[test]
    fn from_vtt_should_parse_cues_with_and_without_identifiers() {
        let track = SubtitleTrack::from_vtt(VTT_SAMPLE).unwrap();
        assert_eq!(track.events.len(), 2);
        let ev = &track.events[0];
        assert_eq!(ev.start, Duration::from_millis(1_000));
        assert_eq!(ev.end, Duration::from_millis(4_000));
        assert_eq!(ev.text, "Hello world");
    }

    #[test]
    fn from_vtt_should_strip_voice_tags_preserving_raw() {
        let track = SubtitleTrack::from_vtt(VTT_SAMPLE).unwrap();
        let ev = &track.events[1];
        assert_eq!(ev.text, "Voice tagged text");
        assert_eq!(ev.raw, "<v Speaker>Voice tagged text</v>");
    }

    #[test]
    fn from_vtt_should_ignore_cue_settings_in_timestamp_line() {
        let track = SubtitleTrack::from_vtt(VTT_SAMPLE).unwrap();
        // Second cue has "align:center" setting — end must still parse correctly.
        assert_eq!(track.events[1].end, Duration::from_millis(7_000));
    }

    #[test]
    fn from_vtt_should_return_parse_error_for_missing_header() {
        let result = SubtitleTrack::from_vtt("not a vtt file\ncontent");
        assert!(matches!(result, Err(SubtitleError::ParseError { .. })));
    }

    #[test]
    fn from_vtt_should_return_no_events_for_empty_content() {
        let result = SubtitleTrack::from_vtt("WEBVTT\n\n");
        assert!(matches!(result, Err(SubtitleError::NoEvents)));
    }

    // ── from_file ─────────────────────────────────────────────────────────────

    #[test]
    fn from_file_should_return_unsupported_for_unknown_extension() {
        let result = SubtitleTrack::from_file("subtitle.xyz");
        assert!(matches!(
            result,
            Err(SubtitleError::UnsupportedFormat { .. })
        ));
    }

    // ── timestamp helpers ─────────────────────────────────────────────────────

    #[test]
    fn parse_srt_timestamp_should_parse_millisecond_precision() {
        let ts = parse_srt_timestamp("01:23:45,678").unwrap();
        let expected_ms = 1 * 3_600_000 + 23 * 60_000 + 45 * 1_000 + 678;
        assert_eq!(ts, Duration::from_millis(expected_ms));
    }

    #[test]
    fn parse_srt_timestamp_should_parse_zero_timestamp() {
        let ts = parse_srt_timestamp("00:00:00,000").unwrap();
        assert_eq!(ts, Duration::from_millis(0));
    }

    #[test]
    fn parse_ass_timestamp_should_parse_centisecond_precision() {
        let ts = parse_ass_timestamp("1:23:45.67").unwrap();
        let expected_ms = 1 * 3_600_000 + 23 * 60_000 + 45 * 1_000 + 670;
        assert_eq!(ts, Duration::from_millis(expected_ms));
    }

    #[test]
    fn parse_vtt_timestamp_should_accept_mm_ss_format() {
        let ts = parse_vtt_timestamp("05:30.500").unwrap();
        assert_eq!(ts, Duration::from_millis(5 * 60_000 + 30 * 1_000 + 500));
    }

    #[test]
    fn parse_vtt_timestamp_should_accept_hh_mm_ss_format() {
        let ts = parse_vtt_timestamp("01:02:03.456").unwrap();
        let expected_ms = 3_600_000 + 2 * 60_000 + 3 * 1_000 + 456;
        assert_eq!(ts, Duration::from_millis(expected_ms));
    }

    // ── tag stripping helpers ─────────────────────────────────────────────────

    #[test]
    fn strip_html_tags_should_remove_italic_bold_underline() {
        assert_eq!(strip_html_tags("<i>italic</i>"), "italic");
        assert_eq!(strip_html_tags("<b>bold</b>"), "bold");
        assert_eq!(strip_html_tags("<u>under</u>"), "under");
    }

    #[test]
    fn strip_html_tags_should_remove_voice_span() {
        assert_eq!(strip_html_tags("<v Speaker>text</v>"), "text");
    }

    #[test]
    fn strip_ass_tags_should_remove_curly_brace_overrides() {
        assert_eq!(strip_ass_tags("{\\an8}text"), "text");
        assert_eq!(strip_ass_tags("before{\\pos(100,200)}after"), "beforeafter");
    }

    #[test]
    fn strip_ass_tags_should_convert_soft_line_breaks() {
        assert_eq!(strip_ass_tags("line1\\Nline2"), "line1\nline2");
        assert_eq!(strip_ass_tags("line1\\nline2"), "line1\nline2");
    }
}
