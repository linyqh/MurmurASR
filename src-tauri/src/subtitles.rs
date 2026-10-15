//! Subtitle layout is derived from the original transcript, never from ASR chunk size.
//! Keep source text and word times intact so old recordings can be reflowed locally.

use serde::{Deserialize, Serialize};

use crate::engine::{ffi::Word, Segment};

const MAX_SECONDS: f64 = 7.0;
const LONG_PAUSE: f64 = 1.2;
const LINE_WIDTH: f64 = 42.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cue {
    pub index: usize,
    pub start: f64,
    pub end: f64,
    /// Explicit line breaks are shared by the preview, SRT and VTT exports.
    pub text: String,
    pub words: Vec<Word>,
    /// "word" or "segment"; segment timings are a conservative fallback.
    pub timing: String,
    pub issues: Vec<String>,
}

#[derive(Clone)]
struct Unit {
    text: String,
    word: Word,
}

fn cjk(c: char) -> bool {
    matches!(c, '\u{2e80}'..='\u{9fff}' | '\u{ac00}'..='\u{d7af}' | '\u{f900}'..='\u{faff}' | '\u{ff01}'..='\u{ff60}' | '\u{20000}'..='\u{3134f}')
}

fn width(text: &str) -> f64 {
    text.chars()
        .map(|c| if cjk(c) { 42.0 / 16.0 } else { 1.0 })
        .sum()
}

fn reading_seconds(text: &str) -> f64 {
    text.chars()
        .filter(|c| *c != '\n')
        .map(|c| if cjk(c) { 1.0 / 9.0 } else { 1.0 / 20.0 })
        .sum()
}

fn normalized(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn terminal(text: &str) -> char {
    text.trim_end()
        .trim_end_matches(['"', '\'', '”', '’', '」', '』', ')', '）'])
        .chars()
        .last()
        .unwrap_or(' ')
}

fn sentence_end(text: &str) -> bool {
    if matches!(
        text.trim().to_lowercase().as_str(),
        "mr." | "mrs." | "ms." | "dr." | "prof." | "e.g." | "i.e."
    ) {
        return false;
    }
    matches!(terminal(text), '.' | '!' | '?' | '。' | '！' | '？' | '…')
}

fn boundary_cost(left: &str, right: &str) -> f64 {
    let a = normalized(left);
    let b = normalized(right);
    // A decimal point between number tokens is not a sentence boundary.
    if !a.is_empty()
        && !b.is_empty()
        && a.chars().all(|c| c.is_numeric())
        && b.chars().all(|c| c.is_numeric())
    {
        return 12.0;
    }
    if sentence_end(left) {
        return 0.0;
    }
    if matches!(terminal(left), ',' | ';' | ':' | '，' | '；' | '：' | '、') {
        return 1.0;
    }
    let contracted = [
        "'s", "'re", "'m", "'ll", "'ve", "'d", "’s", "’re", "’m", "’ll", "’ve", "’d",
    ]
    .iter()
    .any(|suffix| left.trim_end().to_lowercase().ends_with(suffix));
    // Avoid stranding articles, auxiliaries, negation, or a number before its unit.
    if contracted
        || matches!(
            a.as_str(),
            "i" | "you"
                | "he"
                | "she"
                | "it"
                | "we"
                | "they"
                | "a"
                | "an"
                | "the"
                | "my"
                | "your"
                | "his"
                | "her"
                | "our"
                | "their"
                | "to"
                | "of"
                | "in"
                | "on"
                | "at"
                | "for"
                | "with"
                | "and"
                | "or"
                | "but"
                | "not"
                | "dont"
                | "doesnt"
                | "cant"
                | "will"
                | "would"
                | "could"
                | "should"
                | "have"
                | "has"
                | "is"
                | "are"
                | "was"
                | "were"
                | "going"
                | "mr"
                | "mrs"
                | "ms"
                | "dr"
                | "prof"
        )
        || (!a.is_empty()
            && a.chars().all(|c| c.is_numeric())
            && matches!(
                b.as_str(),
                "kg" | "km"
                    | "cm"
                    | "m"
                    | "seconds"
                    | "minutes"
                    | "years"
                    | "元"
                    | "年"
                    | "月"
                    | "日"
                    | "个"
                    | "米"
                    | "秒"
            ))
    {
        return 12.0;
    }
    if matches!(
        b.as_str(),
        "and" | "but" | "because" | "so" | "when" | "如果" | "但是"
    ) {
        2.0
    } else {
        4.0
    }
}

/// Recover original spelling and punctuation using a complete, monotonic text match.
/// Partial/mismatched alignments must not invent timings for the unmatched text.
fn aligned_units(segment: &Segment) -> Option<Vec<Unit>> {
    if segment.words.is_empty() {
        return None;
    }
    let mut letters = Vec::new();
    for (at, c) in segment
        .text
        .char_indices()
        .filter(|(_, c)| c.is_alphanumeric())
    {
        for lower in c.to_lowercase() {
            letters.push((lower, at, at + c.len_utf8()));
        }
    }
    let mut cursor = 0;
    let mut ranges = Vec::new();
    let mut previous_end = segment.start;
    for word in &segment.words {
        if !word.start.is_finite()
            || !word.end.is_finite()
            || word.end <= word.start
            || word.start < previous_end - 0.001
            || word.start < segment.start - 0.25
            || word.end > segment.end + 0.25
        {
            return None;
        }
        let key: Vec<_> = normalized(&word.text).chars().collect();
        if key.is_empty() || cursor + key.len() > letters.len() {
            return None;
        }
        if !key
            .iter()
            .zip(&letters[cursor..cursor + key.len()])
            .all(|(a, b)| *a == b.0)
        {
            return None;
        }
        ranges.push((letters[cursor].1, letters[cursor + key.len() - 1].2));
        cursor += key.len();
        previous_end = word.end;
    }
    if cursor != letters.len() {
        return None;
    }
    let mut from = 0;
    let mut units = Vec::new();
    let mut previous_end = segment.start;
    for (i, word) in segment.words.iter().enumerate() {
        let to = if i + 1 == ranges.len() {
            segment.text.len()
        } else {
            let end = ranges[i].1;
            let gap = &segment.text[end..ranges[i + 1].0];
            end + gap
                .find(|c: char| c.is_whitespace() || matches!(c, '(' | '（' | '「' | '『' | '“'))
                .unwrap_or(gap.len())
        };
        let mut word = word.clone();
        // Tolerate sub-millisecond rounding differences without overlapping words.
        word.start = word.start.max(segment.start).max(previous_end);
        word.end = word.end.min(segment.end);
        if word.end <= word.start {
            return None;
        }
        previous_end = word.end;
        units.push(Unit {
            text: segment.text[from..to].to_string(),
            word,
        });
        from = to;
    }
    Some(units)
}

fn text_of(units: &[Unit]) -> String {
    units
        .iter()
        .map(|u| u.text.as_str())
        .collect::<String>()
        .trim()
        .replace(['\r', '\n'], " ")
}

/// Two-line layout; grammar takes priority over perfectly balanced line lengths.
fn layout(units: &[Unit], max_lines: u8) -> (String, f64) {
    let text = text_of(units);
    if max_lines == 1 || width(&text) <= LINE_WIDTH || units.len() == 1 {
        return (text, 0.0);
    }
    let mut best = (f64::INFINITY, 1);
    for at in 1..units.len() {
        let left = text_of(&units[..at]);
        let right = text_of(&units[at..]);
        let (a, b) = (width(&left), width(&right));
        let overflow = (a - LINE_WIDTH).max(0.0) + (b - LINE_WIDTH).max(0.0);
        let score = overflow * 100.0
            + boundary_cost(&units[at - 1].text, &units[at].text)
            + (a - b).abs() * 0.04
            + if a < 6.0 || b < 6.0 { 5.0 } else { 0.0 };
        if score < best.0 {
            best = (score, at);
        }
    }
    (
        format!(
            "{}\n{}",
            text_of(&units[..best.1]),
            text_of(&units[best.1..])
        ),
        best.0,
    )
}

fn display_end(units: &[Unit], next: Option<&Unit>, duration: f64) -> f64 {
    let start = units[0].word.start;
    let spoken_end = units.last().unwrap().word.end;
    let desired = (spoken_end + 0.15)
        .max(start + 1.0)
        .max(start + reading_seconds(&text_of(units)))
        .min(spoken_end + 0.8)
        .min(start + MAX_SECONDS);
    let limit = next.map_or(duration, |u| u.word.start.min(duration));
    desired.min(limit).max(spoken_end.min(limit))
}

fn diagnose(cue: &mut Cue) {
    let duration = cue.end - cue.start;
    if cue.text.lines().any(|line| width(line) > LINE_WIDTH + 0.01) {
        cue.issues.push("line_length".into());
    }
    if duration > MAX_SECONDS + 0.001 {
        cue.issues.push("long_duration".into());
    }
    if duration < 0.833 {
        cue.issues.push("short_duration".into());
    }
    if reading_seconds(&cue.text) > duration + 0.001 {
        cue.issues.push("reading_speed".into());
    }
}

fn partition(units: &[Unit], duration: f64, max_lines: u8, out: &mut Vec<Cue>) {
    if units.is_empty() {
        return;
    }
    let n = units.len();
    let mut costs = vec![f64::INFINITY; n + 1];
    let mut ends = vec![0; n];
    costs[n] = 0.0;
    for i in (0..n).rev() {
        let mut internal_sentences = 0.0;
        let mut sentence_start = i;
        for j in i + 1..=n {
            let part = &units[i..j];
            let spoken = part.last().unwrap().word.end - part[0].word.start;
            let text = text_of(part);
            if j > i + 1
                && (spoken > MAX_SECONDS || width(&text) > LINE_WIDTH * f64::from(max_lines))
            {
                break;
            }
            if j > i + 1 && sentence_end(&units[j - 2].text) {
                internal_sentences += if width(&text_of(&units[sentence_start..j - 1])) <= 18.0 {
                    1.0
                } else {
                    6.0
                };
                sentence_start = j - 1;
            }
            let end = display_end(part, units.get(j), duration);
            let seconds = (end - part[0].word.start).max(0.01);
            let (lines, line_cost) = layout(part, max_lines);
            // A two-line candidate that cannot actually fit is not feasible.
            if j > i + 1 && lines.lines().any(|l| width(l) > LINE_WIDTH + 0.01) {
                continue;
            }
            let boundary = units.get(j).map_or(0.0, |next| {
                let pause = (next.word.start - part.last().unwrap().word.end).max(0.0);
                boundary_cost(&part.last().unwrap().text, &next.text)
                    * (1.0 - pause / LONG_PAUSE).max(0.0)
            });
            let speed_penalty = ((reading_seconds(&text) / seconds - 1.0).max(0.0)).powi(2) * 15.0;
            let score = 2.0
                + boundary
                + internal_sentences
                + line_cost * 0.15
                + (1.4 - seconds).max(0.0).powi(2) * 6.0
                + (seconds - 5.0).max(0.0).powi(2) * 0.8
                + speed_penalty
                + costs[j];
            if score < costs[i] {
                costs[i] = score;
                ends[i] = j;
            }
        }
    }
    let mut i = 0;
    while i < n {
        let j = ends[i].max(i + 1);
        let part = &units[i..j];
        let mut cue = Cue {
            index: out.len(),
            start: part[0].word.start,
            end: display_end(part, units.get(j), duration),
            text: layout(part, max_lines).0,
            words: part.iter().map(|u| u.word.clone()).collect(),
            timing: "word".into(),
            issues: Vec::new(),
        };
        diagnose(&mut cue);
        out.push(cue);
        i = j;
    }
}

/// Tokenization is used only for line wrapping when precise timings are unavailable.
fn untimed_units(text: &str) -> Vec<Unit> {
    let mut cuts = vec![0];
    let mut previous_cjk = false;
    for (at, c) in text.char_indices() {
        if at > 0 && (c.is_whitespace() || (c.is_alphanumeric() && (cjk(c) || previous_cjk))) {
            cuts.push(at);
        }
        if !c.is_whitespace() {
            previous_cjk = cjk(c);
        }
    }
    cuts.push(text.len());
    cuts.windows(2)
        .filter(|w| w[0] < w[1])
        .map(|w| Unit {
            text: text[w[0]..w[1]].to_string(),
            word: Word {
                text: String::new(),
                start: 0.0,
                end: 0.0,
            },
        })
        .collect()
}

/// Reflow all reliable words together, including across ASR chunk boundaries.
/// Missing/invalid alignments retain their source time range and are explicitly flagged.
pub fn compose(segments: &[Segment], duration: f64, max_lines: u8) -> Vec<Cue> {
    let max_lines = max_lines.clamp(1, 2);
    if !duration.is_finite() || duration <= 0.0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut pending: Vec<Unit> = Vec::new();
    let mut previous_end: f64 = 0.0;
    for segment in segments {
        if segment.text.trim().is_empty() {
            continue;
        }
        if !segment.start.is_finite() || !segment.end.is_finite() {
            continue;
        }
        let start = segment.start.max(0.0).max(previous_end);
        let end = segment.end.min(duration);
        if end <= start {
            continue;
        }
        let valid_order = segment.start >= previous_end - 0.001;
        let mut bounded = segment.clone();
        bounded.start = start;
        bounded.end = end;
        if let Some(mut units) = valid_order.then(|| aligned_units(&bounded)).flatten() {
            if let (Some(last), Some(first)) = (pending.last(), units.first_mut()) {
                if !last.text.ends_with(char::is_whitespace)
                    && !first.text.starts_with(char::is_whitespace)
                    && !last.text.chars().last().is_some_and(cjk)
                    && !first.text.chars().next().is_some_and(cjk)
                {
                    first.text.insert(0, ' ');
                }
            }
            for unit in units {
                if pending.last().is_some_and(|last| {
                    let gap = unit.word.start - last.word.end;
                    (gap >= 0.6 && sentence_end(&last.text))
                        || (gap >= LONG_PAUSE && boundary_cost(&last.text, &unit.text) < 8.0)
                        || gap >= 3.0
                }) {
                    partition(&pending, unit.word.start.min(duration), max_lines, &mut out);
                    pending.clear();
                }
                pending.push(unit);
            }
        } else {
            partition(&pending, start, max_lines, &mut out);
            pending.clear();
            let mut cue = Cue {
                index: out.len(),
                start,
                end,
                text: layout(&untimed_units(segment.text.trim()), max_lines).0,
                words: Vec::new(),
                timing: "segment".into(),
                issues: vec!["segment_timing".into()],
            };
            diagnose(&mut cue);
            out.push(cue);
        }
        previous_end = end;
    }
    partition(&pending, duration, max_lines, &mut out);
    out
}
