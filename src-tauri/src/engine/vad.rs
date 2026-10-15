//! Voice activity segmentation built on the engine's streaming Silero VAD.

use super::ffi::{Session, VoiceActivity};

/// Silero's preferred block size at 16 kHz.
pub const VAD_BLOCK: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn samples(&self) -> usize {
        self.end - self.start
    }
}

/// Runs VAD over a full buffer and returns raw speech spans in samples.
pub fn detect_speech(
    vad: &mut Session,
    samples: &[f32],
    rate: u32,
    mut cancelled: impl FnMut(usize) -> bool,
) -> super::ffi::Result<Vec<Span>> {
    vad.stream_start()?;
    let mut spans = Vec::new();
    let mut open: Option<usize> = None;
    let mut offset = 0usize;
    while offset + VAD_BLOCK <= samples.len() {
        if cancelled(offset) {
            break;
        }
        for activity in vad.push_vad(&samples[offset..offset + VAD_BLOCK], rate, offset as i64)? {
            match activity {
                VoiceActivity::Start(s) => {
                    if open.is_none() {
                        open = Some(s.max(0) as usize);
                    }
                }
                VoiceActivity::End(e) => {
                    if let Some(start) = open.take() {
                        let end = (e.max(0) as usize).min(samples.len());
                        if end > start {
                            spans.push(Span { start, end });
                        }
                    }
                }
            }
        }
        offset += VAD_BLOCK;
    }
    if let Some(start) = open {
        spans.push(Span {
            start,
            end: samples.len(),
        });
    }
    let _ = vad.stream_finish();
    Ok(spans)
}

/// Pads speech spans, merges ones separated by short gaps and packs them into
/// chunks no longer than `max_len`, splitting overly long spans evenly.
pub fn plan_chunks(
    spans: &[Span],
    total: usize,
    pad: usize,
    merge_gap: usize,
    max_len: usize,
) -> Vec<Span> {
    let mut padded: Vec<Span> = spans
        .iter()
        .map(|s| Span {
            start: s.start.saturating_sub(pad),
            end: (s.end + pad).min(total),
        })
        .collect();
    padded.sort_by_key(|s| s.start);

    let mut chunks: Vec<Span> = Vec::new();
    for span in padded {
        if let Some(last) = chunks.last_mut() {
            let gap = span.start.saturating_sub(last.end);
            if gap <= merge_gap && span.end.max(last.end) - last.start <= max_len {
                last.end = last.end.max(span.end);
                continue;
            }
        }
        chunks.push(span);
    }

    // Padding may push a chunk into its neighbour; split the difference so
    // adjacent chunks never share audio (and subtitle cues never overlap).
    for i in 1..chunks.len() {
        if chunks[i].start < chunks[i - 1].end {
            let mid = (chunks[i].start + chunks[i - 1].end) / 2;
            chunks[i - 1].end = mid;
            chunks[i].start = mid;
        }
    }

    let mut out = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        if chunk.samples() <= max_len {
            out.push(chunk);
            continue;
        }
        let parts = chunk.samples().div_ceil(max_len);
        let step = chunk.samples().div_ceil(parts);
        let mut start = chunk.start;
        while start < chunk.end {
            let end = (start + step).min(chunk.end);
            out.push(Span { start, end });
            start = end;
        }
    }
    out
}

/// Incremental endpointing for live capture. Feed 16 kHz audio; it reports when
/// an utterance starts and when one has ended (after trailing silence or when
/// it grows past the maximum length).
pub struct Endpointer {
    pending: Vec<f32>,
    position: usize,
    in_speech: bool,
    speech_start: usize,
    last_end: Option<usize>,
    silence_hold: usize,
    max_len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointEvent {
    SpeechStart(usize),
    /// A finished utterance; `forced` when cut because it grew too long.
    Utterance {
        span: Span,
        forced: bool,
    },
}

impl Endpointer {
    pub fn new(silence_hold: usize, max_len: usize) -> Self {
        Self {
            pending: Vec::new(),
            position: 0,
            in_speech: false,
            speech_start: 0,
            last_end: None,
            silence_hold,
            max_len,
        }
    }

    pub fn in_speech(&self) -> bool {
        self.in_speech
    }

    pub fn speech_start(&self) -> usize {
        self.speech_start
    }

    /// `position` is the absolute sample index of the first sample of `audio`.
    pub fn feed(
        &mut self,
        vad: &mut Session,
        audio: &[f32],
    ) -> super::ffi::Result<Vec<EndpointEvent>> {
        self.pending.extend_from_slice(audio);
        let mut events = Vec::new();
        let mut consumed = 0;
        while self.pending.len() - consumed >= VAD_BLOCK {
            let block = &self.pending[consumed..consumed + VAD_BLOCK];
            let activity = vad.push_vad(
                block,
                super::super::audio::TARGET_RATE,
                self.position as i64,
            )?;
            consumed += VAD_BLOCK;
            self.position += VAD_BLOCK;
            for a in activity {
                match a {
                    VoiceActivity::Start(s) => {
                        if !self.in_speech {
                            self.in_speech = true;
                            self.speech_start = s.max(0) as usize;
                            self.last_end = None;
                            events.push(EndpointEvent::SpeechStart(self.speech_start));
                        } else {
                            // Speech resumed before the hold expired: same utterance.
                            self.last_end = None;
                        }
                    }
                    VoiceActivity::End(e) => {
                        if self.in_speech {
                            self.last_end = Some(e.max(0) as usize);
                        }
                    }
                }
            }
            if self.in_speech {
                if let Some(end) = self.last_end {
                    if self.position.saturating_sub(end) >= self.silence_hold {
                        events.push(EndpointEvent::Utterance {
                            span: Span {
                                start: self.speech_start,
                                end,
                            },
                            forced: false,
                        });
                        self.in_speech = false;
                        self.last_end = None;
                    }
                } else if self.position - self.speech_start >= self.max_len {
                    events.push(EndpointEvent::Utterance {
                        span: Span {
                            start: self.speech_start,
                            end: self.position,
                        },
                        forced: true,
                    });
                    self.speech_start = self.position;
                }
            }
        }
        self.pending.drain(..consumed);
        Ok(events)
    }

    /// Closes any open utterance at `end`.
    pub fn flush(&mut self, end: usize) -> Option<Span> {
        if !self.in_speech {
            return None;
        }
        self.in_speech = false;
        let stop = self.last_end.take().unwrap_or(end);
        (stop > self.speech_start).then_some(Span {
            start: self.speech_start,
            end: stop,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(start: usize, end: usize) -> Span {
        Span { start, end }
    }

    #[test]
    fn merges_close_spans() {
        let chunks = plan_chunks(&[s(100, 200), s(250, 400)], 1000, 10, 100, 1000);
        assert_eq!(chunks, vec![s(90, 410)]);
    }

    #[test]
    fn keeps_distant_spans_apart() {
        let chunks = plan_chunks(&[s(100, 200), s(600, 700)], 1000, 0, 100, 1000);
        assert_eq!(chunks, vec![s(100, 200), s(600, 700)]);
    }

    #[test]
    fn respects_max_len_when_merging() {
        let chunks = plan_chunks(&[s(0, 300), s(310, 600)], 1000, 0, 100, 400);
        assert_eq!(chunks, vec![s(0, 300), s(310, 600)]);
    }

    #[test]
    fn splits_long_spans_evenly() {
        let chunks = plan_chunks(&[s(0, 1000)], 1000, 0, 0, 400);
        assert_eq!(chunks, vec![s(0, 334), s(334, 668), s(668, 1000)]);
        assert!(chunks.iter().all(|c| c.samples() <= 400));
    }

    #[test]
    fn padded_neighbours_never_overlap() {
        // Too long to merge, close enough that padding would overlap.
        let chunks = plan_chunks(&[s(0, 300), s(320, 600)], 1000, 50, 100, 400);
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].end <= chunks[1].start, "{chunks:?}");
        assert_eq!(chunks[0].start, 0);
        assert_eq!(chunks[1].end, 650);
    }

    #[test]
    fn clamps_padding_to_bounds() {
        let chunks = plan_chunks(&[s(5, 995)], 1000, 50, 0, 2000);
        assert_eq!(chunks, vec![s(0, 1000)]);
    }
}
