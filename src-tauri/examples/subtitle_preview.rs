//! Reflow an existing JSON transcript without inference or changing its source file.
//! cargo run --example subtitle_preview -- /path/transcript.json [1|2] > /tmp/preview.json
use murmur_lib::{engine::Segment, subtitles::compose};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("expected transcript JSON path")?;
    let mut value: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
    let segments: Vec<Segment> = serde_json::from_value(value["segments"].clone())?;
    let duration = value["duration"].as_f64().ok_or("missing duration")?;
    let max_lines: u8 = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "1".into())
        .parse()?;
    if !matches!(max_lines, 1 | 2) {
        return Err("subtitle lines must be 1 or 2".into());
    }
    value["subtitleMaxLines"] = max_lines.into();
    value["subtitles"] = serde_json::to_value(compose(&segments, duration, max_lines))?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
