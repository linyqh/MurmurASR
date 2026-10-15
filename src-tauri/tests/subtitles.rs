use murmur_lib::engine::{ffi::Word, Segment};
use murmur_lib::subtitles::{compose, Cue};

fn segment(text: &str, start: f64, step: f64) -> Segment {
    let words: Vec<_> = text
        .split_whitespace()
        .enumerate()
        .map(|(i, s)| Word {
            text: s.trim_matches(|c: char| !c.is_alphanumeric()).to_string(),
            start: start + i as f64 * step,
            end: start + (i as f64 + 0.8) * step,
        })
        .collect();
    Segment {
        index: 0,
        start,
        end: words.last().unwrap().end + 0.2,
        text: text.into(),
        words,
    }
}

fn compact(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn check(source: &[Segment], cues: &[Cue], duration: f64) {
    assert_eq!(
        compact(&source.iter().map(|s| s.text.as_str()).collect::<String>()),
        compact(&cues.iter().map(|s| s.text.as_str()).collect::<String>())
    );
    assert!(cues
        .iter()
        .all(|c| c.start >= 0.0 && c.end > c.start && c.end <= duration));
    assert!(cues.windows(2).all(|c| c[0].end <= c[1].start));
    assert!(cues.iter().all(|c| c.text.lines().count() <= 2));
    assert!(cues.iter().enumerate().all(|(i, c)| c.index == i));
    assert!(cues
        .iter()
        .all(|c| c.words.iter().all(|w| w.start >= c.start && w.end <= c.end)));
}

#[test]
fn long_asr_chunk_becomes_readable_sentences() {
    let s = segment("This is why these animals are dangerous. This is a clear example. I know someone will ask about the water dish. He has nothing in there. You are right. Look at him. He will do everything he can to destroy that cage right now. I will wait until he calms down. Yeah. Yeah.", 1.0, 0.4);
    let cues = compose(std::slice::from_ref(&s), 30.0, 2);
    check(&[s], &cues, 30.0);
    assert!(cues.len() >= 7, "{cues:#?}");
    assert_eq!(cues[0].text, "This is why these animals are dangerous.");
    assert!(cues
        .iter()
        .all(|c| c.end - c.start <= 7.0 && c.text.lines().all(|l| l.chars().count() <= 42)));
}

#[test]
fn short_fragments_can_join_across_asr_chunks_but_long_pauses_stay_empty() {
    let a = segment("So", 1.0, 0.2);
    let b = segment("we should leave now.", 1.5, 0.3);
    let c = segment("Yes!", 8.0, 0.2);
    let cues = compose(&[a.clone(), b.clone(), c.clone()], 10.0, 2);
    check(&[a, b, c], &cues, 10.0);
    assert_eq!(cues.len(), 2, "{cues:#?}");
    assert_eq!(cues[0].text, "So we should leave now.");
    assert!(cues[0].end < 4.0);
    assert_eq!(cues[1].start, 8.0);
}

#[test]
fn cjk_punctuation_and_all_characters_survive() {
    let text = "大家好，欢迎收听今天的节目。今天我们来聊一聊人工智能在语音识别领域的最新进展。我们下期再见！";
    let words: Vec<_> = text
        .chars()
        .filter(|c| c.is_alphanumeric())
        .enumerate()
        .map(|(i, c)| Word {
            text: c.to_string(),
            start: i as f64 * 0.22,
            end: (i as f64 + 0.8) * 0.22,
        })
        .collect();
    let s = Segment {
        index: 0,
        start: 0.0,
        end: 12.0,
        text: text.into(),
        words,
    };
    let cues = compose(std::slice::from_ref(&s), 12.0, 2);
    check(&[s], &cues, 12.0);
    assert!(cues.len() >= 3);
    assert!(cues
        .iter()
        .all(|c| c.text.lines().all(|l| l.chars().count() <= 16)));
}

#[test]
fn no_punctuation_still_splits_without_dropping_words() {
    let s = segment(&vec!["something"; 70].join(" "), 0.0, 0.24);
    let cues = compose(std::slice::from_ref(&s), 20.0, 2);
    check(&[s], &cues, 20.0);
    assert!(cues.len() > 5);
    assert!(cues.iter().all(|c| c.text.lines().all(|l| l.len() <= 42)));
    assert!(cues
        .iter()
        .any(|c| c.issues.iter().any(|i| i == "reading_speed")));
}

#[test]
fn punctuation_normalization_accepts_contractions_and_merged_aligner_tokens() {
    let mut s = segment("I don't know why life—we agree—is hard.", 0.0, 0.5);
    s.words[1].text = "dont".into();
    s.words[4].text = "lifewe".into();
    s.words[5].text = "agreeis".into();
    let cues = compose(std::slice::from_ref(&s), 5.0, 2);
    check(&[s], &cues, 5.0);
    assert!(cues.iter().all(|c| c.timing == "word"));
}

#[test]
fn missing_mismatched_and_invalid_timings_fall_back_without_fabrication() {
    for variant in 0..4 {
        let mut s = segment("This sentence must remain intact.", 1.0, 0.5);
        match variant {
            0 => s.words.clear(),
            1 => {
                s.words.remove(2);
            }
            2 => s.words[2].start = 0.0,
            _ => s.words[0].end = f64::NAN,
        }
        let cues = compose(std::slice::from_ref(&s), 8.0, 2);
        check(std::slice::from_ref(&s), &cues, 8.0);
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].start, s.start);
        assert_eq!(cues[0].end, s.end);
        assert_eq!(cues[0].timing, "segment");
        assert!(cues[0].words.is_empty());
    }
}

#[test]
fn unbreakable_tokens_are_preserved_and_reported() {
    let s = segment(&"x".repeat(110), 0.0, 1.0);
    let cues = compose(std::slice::from_ref(&s), 2.0, 2);
    check(&[s], &cues, 2.0);
    assert!(cues[0].issues.contains(&"line_length".into()));
}

#[test]
fn empty_input_and_clip_boundaries_are_safe() {
    assert!(compose(&[], 1.0, 2).is_empty());
    let s = segment("Hello", 0.0, 0.5);
    let cues = compose(std::slice::from_ref(&s), 0.35, 2);
    check(&[s], &cues, 0.35);
    assert_eq!(cues[0].end, 0.35);
}

#[test]
fn hesitation_after_subject_does_not_attach_it_to_previous_sentence() {
    let mut s = segment("We should leave now. I have another idea.", 0.0, 0.4);
    for w in &mut s.words[5..] {
        w.start += 1.5;
        w.end += 1.5;
    }
    s.end += 1.5;
    let cues = compose(std::slice::from_ref(&s), 8.0, 2);
    check(&[s], &cues, 8.0);
    assert!(
        cues.iter().any(|c| c.text.starts_with("I have")),
        "{cues:#?}"
    );
    assert!(cues.iter().all(|c| !c.text.ends_with(" I")));
}

#[test]
fn edited_newlines_do_not_create_extra_subtitle_lines() {
    let s = Segment {
        index: 0,
        start: 0.0,
        end: 5.0,
        text: "Hello\nthere.\nWelcome\nback.".into(),
        words: vec![],
    };
    let cues = compose(std::slice::from_ref(&s), 5.0, 2);
    check(&[s], &cues, 5.0);
}

#[test]
fn submillisecond_alignment_overlap_is_clamped() {
    let mut s = segment("Hello there. Welcome back.", 0.0, 0.5);
    s.words[2].start = s.words[1].end - 0.0005;
    let cues = compose(std::slice::from_ref(&s), 4.0, 2);
    check(&[s], &cues, 4.0);
    assert!(cues.iter().all(|c| c.timing == "word"));
    let words: Vec<_> = cues.iter().flat_map(|c| &c.words).collect();
    assert!(words.windows(2).all(|p| p[0].end <= p[1].start));
}

#[test]
fn one_line_resegments_words_instead_of_just_removing_line_breaks() {
    let s = segment(
        "We should keep all of these words together when two lines are available.",
        0.0,
        0.25,
    );
    let one = compose(std::slice::from_ref(&s), 5.0, 1);
    let two = compose(std::slice::from_ref(&s), 5.0, 2);
    check(std::slice::from_ref(&s), &one, 5.0);
    check(std::slice::from_ref(&s), &two, 5.0);
    assert!(one.len() > two.len(), "one={one:#?}, two={two:#?}");
    assert!(one
        .iter()
        .all(|c| !c.text.contains('\n') && c.text.chars().count() <= 42));
    assert!(two.iter().any(|c| c.text.contains('\n')));
}

#[test]
fn cjk_one_line_limit_preserves_text_and_times() {
    let text = "今天我们来聊一聊人工智能在语音识别领域的最新进展。";
    let words = text
        .chars()
        .filter(|c| c.is_alphanumeric())
        .enumerate()
        .map(|(i, c)| Word {
            text: c.to_string(),
            start: i as f64 * 0.2,
            end: (i as f64 + 0.9) * 0.2,
        })
        .collect();
    let s = Segment {
        index: 0,
        start: 0.0,
        end: 6.0,
        text: text.into(),
        words,
    };
    let cues = compose(std::slice::from_ref(&s), 6.0, 1);
    check(&[s], &cues, 6.0);
    assert!(cues
        .iter()
        .all(|c| c.text.lines().count() == 1 && c.text.chars().count() <= 16));
}

#[test]
fn untimed_fallback_respects_line_setting_without_inventing_timestamps() {
    let mut s = segment(
        "These are some longer words that cannot be timed individually without reliable alignment.",
        0.0,
        0.2,
    );
    s.words.clear();
    for lines in [1, 2] {
        let cues = compose(std::slice::from_ref(&s), 5.0, lines);
        assert_eq!(cues.len(), 1);
        assert!(cues[0].text.lines().count() <= usize::from(lines));
        assert_eq!((cues[0].start, cues[0].end), (s.start, s.end));
    }
}
