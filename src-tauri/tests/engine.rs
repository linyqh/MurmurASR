//! End-to-end checks against the real engine and worker. They need the staged
//! engine (`npm run engine:build`) and a downloaded model, so they skip otherwise:
//!
//! MURMUR_TEST_MODEL=/path/model.gguf MURMUR_TEST_FAMILY=qwen3_asr \
//! MURMUR_TEST_AUDIO=/path/speech.wav cargo test --test engine -- --nocapture
//!
//! Set MURMUR_TEST_ALIGNER=/path/qwen3-forced-aligner.gguf to also require word
//! timestamps.

use std::path::PathBuf;
use std::time::Instant;

use murmur_lib::engine::ffi::EngineLayout;
use murmur_lib::engine::testing::run_file;
use murmur_lib::engine::EngineEvent;

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .map(PathBuf::from)
        .filter(|p| p.exists())
}

#[test]
fn transcribes_file_through_worker() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/engine");
    let layout = EngineLayout::from_root(&root);
    let (Some(model), Some(audio)) = (env_path("MURMUR_TEST_MODEL"), env_path("MURMUR_TEST_AUDIO"))
    else {
        eprintln!("skipping: set MURMUR_TEST_MODEL and MURMUR_TEST_AUDIO");
        return;
    };
    if !layout.library.exists() {
        eprintln!(
            "skipping: engine not staged at {}",
            layout.library.display()
        );
        return;
    }
    let family = std::env::var("MURMUR_TEST_FAMILY").unwrap_or_else(|_| "qwen3_asr".into());
    let backend = std::env::var("MURMUR_TEST_BACKEND").unwrap_or_else(|_| "metal".into());
    let language = std::env::var("MURMUR_TEST_LANGUAGE").ok();
    let aligner = env_path("MURMUR_TEST_ALIGNER");

    let started = Instant::now();
    let events = run_file(
        layout,
        &model,
        &family,
        aligner.as_deref(),
        &backend,
        &audio,
        language,
    )
    .expect("worker run");
    let elapsed = started.elapsed().as_secs_f64();

    let mut loaded = false;
    let mut duration = 0.0;
    let mut segments = Vec::new();
    let mut last_progress = 0.0f32;
    for event in &events {
        match event {
            EngineEvent::ModelLoaded { info } => {
                loaded = true;
                eprintln!(
                    "loaded {} on {} in {} ms, languages={}",
                    info.family,
                    info.backend,
                    info.load_ms,
                    info.languages.len()
                );
            }
            EngineEvent::JobStarted { duration: d, .. } => duration = *d,
            EngineEvent::JobSegment {
                segment, progress, ..
            } => {
                assert!(*progress >= last_progress, "progress went backwards");
                last_progress = *progress;
                segments.push(segment.clone());
            }
            _ => {}
        }
    }
    assert!(loaded, "model did not load: {events:?}");
    for s in &segments {
        eprintln!("[{:7.2} - {:7.2}] {}", s.start, s.end, s.text);
        if !s.words.is_empty() {
            let words: Vec<_> = s
                .words
                .iter()
                .map(|w| format!("{}@{:.2}-{:.2}", w.text, w.start, w.end))
                .collect();
            eprintln!("          {}", words.join(" "));
        }
    }
    eprintln!("{duration:.1}s audio, wall {elapsed:.2}s incl. load");
    assert!(!segments.is_empty(), "expected at least one segment");
    assert!(
        segments.windows(2).all(|w| w[0].end <= w[1].start + 1e-6),
        "segments overlap"
    );
    assert!(segments.last().unwrap().end <= duration + 1e-6);
    if aligner.is_some() {
        let words: Vec<_> = segments.iter().flat_map(|s| &s.words).collect();
        assert!(!words.is_empty(), "expected word timestamps");
        assert!(words
            .iter()
            .all(|w| w.start <= w.end && w.end <= duration + 0.5));
        assert!(
            words.windows(2).all(|p| p[0].start <= p[1].start + 1e-6),
            "word starts go backwards"
        );
    }
    if let Ok(expect) = std::env::var("MURMUR_TEST_EXPECT") {
        let joined: String = segments.iter().map(|s| s.text.as_str()).collect();
        assert!(
            joined.contains(&expect),
            "transcript missing {expect:?}: {joined}"
        );
    }
}

#[test]
fn live_endpointer_splits_on_pauses() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/engine");
    let layout = EngineLayout::from_root(&root);
    let Some(audio) = env_path("MURMUR_TEST_AUDIO") else {
        eprintln!("skipping: set MURMUR_TEST_AUDIO");
        return;
    };
    if !layout.library.exists() {
        return;
    }
    let utterances =
        murmur_lib::engine::testing::live_utterances(layout, &audio).expect("endpointing");
    for (s, e) in &utterances {
        eprintln!("utterance {s:6.2} - {e:6.2}");
    }
    assert!(!utterances.is_empty());
    assert!(utterances.windows(2).all(|w| w[0].1 <= w[1].0));
    // Forced cuts land on the next VAD block (512 samples) after 20 s.
    assert!(utterances
        .iter()
        .all(|(s, e)| e - s <= 20.0 + 512.0 / 16_000.0 + 1e-6));
}
