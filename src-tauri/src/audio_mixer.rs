// Real-time mixer for microphone + system audio.
//
// The capture engine delivers mic and ScreenCaptureKit audio on separate
// threads, at their own sample rates and channel counts. Transcription
// providers expect ONE continuous stream, so feeding both callbacks straight
// through splices the two sources end-to-end (doubling the apparent duration,
// breaking timestamps and voice-activity detection).
//
// This mixer normalizes each source to 16 kHz mono, keeps a small per-source
// queue, and emits the sample-aligned sum. A source that stops delivering
// (muted, permission revoked, device unplugged) is dropped from the mix after
// a short grace period so the other keeps flowing with no added latency.

use parking_lot::Mutex;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::capture_engine::AudioSource;

pub const MIX_SAMPLE_RATE: u32 = 16_000;
/// A source that hasn't delivered audio for this long is considered absent.
const STALE_AFTER: Duration = Duration::from_millis(300);
/// Clock drift guard: never let one source run ahead by more than this.
const MAX_LEAD_SAMPLES: usize = MIX_SAMPLE_RATE as usize / 2;

#[derive(Default)]
struct SourceQueue {
    samples: VecDeque<f32>,
    last_seen: Option<Instant>,
    resampler: Resampler,
}

/// Streaming linear resampler to 16 kHz. Carries the fractional read
/// position and last input sample across buffers, so output length is exact
/// over time (per-buffer rounding lost ~0.4% of audio and drifted
/// timestamps by seconds per hour).
#[derive(Default)]
struct Resampler {
    from_rate: u32,
    /// Read position relative to `prev` (index 0 = prev sample)
    pos: f64,
    prev: Option<f32>,
}

impl Resampler {
    fn process(&mut self, mono: &[f32], from_rate: u32) -> Vec<f32> {
        if from_rate == MIX_SAMPLE_RATE || from_rate == 0 {
            return mono.to_vec();
        }
        if from_rate != self.from_rate {
            *self = Resampler { from_rate, ..Default::default() };
        }
        if mono.is_empty() {
            return vec![];
        }
        let step = from_rate as f64 / MIX_SAMPLE_RATE as f64;
        let offset = if self.prev.is_some() { 1 } else { 0 };
        let at = |i: usize| -> f32 {
            if offset == 1 {
                if i == 0 { self.prev.unwrap() } else { mono[i - 1] }
            } else {
                mono[i]
            }
        };
        let last_idx = mono.len() + offset - 1;
        let mut out = Vec::with_capacity((mono.len() as f64 / step) as usize + 2);
        while self.pos <= last_idx as f64 {
            let i = self.pos.floor() as usize;
            let frac = (self.pos - i as f64) as f32;
            let a = at(i);
            let b = if i < last_idx { at(i + 1) } else { a };
            // Only interpolate toward a sample we actually have
            if i == last_idx && frac > 0.0 {
                break;
            }
            out.push(a * (1.0 - frac) + b * frac);
            self.pos += step;
        }
        self.pos -= last_idx as f64;
        self.prev = mono.last().copied();
        out
    }
}

impl SourceQueue {
    fn is_live(&self, now: Instant) -> bool {
        self.last_seen
            .map(|t| now.duration_since(t) < STALE_AFTER)
            .unwrap_or(false)
    }
}

#[derive(Default)]
pub struct AudioMixer {
    state: Mutex<(SourceQueue, SourceQueue)>,
}

impl AudioMixer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a buffer from `source`; any mixed 16 kHz mono audio that is now
    /// ready is handed to `sink` *while the mixer lock is held*, so chunks
    /// from the mic and system-audio threads reach the transcriber in order.
    /// `sink` must not block.
    pub fn push_with(
        &self,
        source: AudioSource,
        samples: &[f32],
        rate: u32,
        channels: u16,
        sink: impl FnOnce(&[f32]),
    ) {
        let mut guard = self.state.lock();
        let out = Self::mix_locked(&mut guard, source, samples, rate, channels);
        if !out.is_empty() {
            sink(&out);
        }
    }

    /// Like `push_with`, returning the ready audio (for tests).
    pub fn push(&self, source: AudioSource, samples: &[f32], rate: u32, channels: u16) -> Vec<f32> {
        let mut guard = self.state.lock();
        Self::mix_locked(&mut guard, source, samples, rate, channels)
    }

    fn mix_locked(
        state: &mut (SourceQueue, SourceQueue),
        source: AudioSource,
        samples: &[f32],
        rate: u32,
        channels: u16,
    ) -> Vec<f32> {
        let now = Instant::now();
        let (mic, sys) = state;

        let q = match source {
            AudioSource::Microphone => &mut *mic,
            AudioSource::System => &mut *sys,
        };
        let mono = downmix(samples, channels);
        let normalized = q.resampler.process(&mono, rate);
        q.samples.extend(normalized);
        q.last_seen = Some(now);

        let (mic_live, sys_live) = (mic.is_live(now), sys.is_live(now));
        let n = match (mic_live, sys_live) {
            (true, true) => mic.samples.len().min(sys.samples.len()),
            (true, false) => mic.samples.len(),
            (false, true) => sys.samples.len(),
            (false, false) => 0,
        };

        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let a = mic.samples.pop_front().unwrap_or(0.0);
            let b = sys.samples.pop_front().unwrap_or(0.0);
            out.push(soft_clip(a + b));
        }

        // Drift / stale-source cleanup
        for q in [&mut *mic, &mut *sys] {
            if !q.is_live(now) {
                q.samples.clear();
            } else if q.samples.len() > MAX_LEAD_SAMPLES {
                let excess = q.samples.len() - MAX_LEAD_SAMPLES;
                q.samples.drain(..excess);
            }
        }

        out
    }
}

/// Gentle limiter so summed sources don't hard-clip.
fn soft_clip(x: f32) -> f32 {
    if x.abs() <= 0.9 {
        x
    } else {
        x.signum() * (0.9 + 0.1 * ((x.abs() - 0.9) * 10.0).tanh())
    }
}

fn downmix(samples: &[f32], channels: u16) -> Vec<f32> {
    let ch = channels.max(1) as usize;
    if ch == 1 {
        return samples.to_vec();
    }
    samples
        .chunks(ch)
        .map(|c| c.iter().sum::<f32>() / c.len() as f32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_source_passes_through() {
        let m = AudioMixer::new();
        let out = m.push(AudioSource::Microphone, &[0.1; 1600], 16_000, 1);
        assert_eq!(out.len(), 1600);
        assert!((out[0] - 0.1).abs() < 1e-6);
    }

    #[test]
    fn two_sources_are_summed_not_concatenated() {
        let m = AudioMixer::new();
        // First mic buffer passes straight through (system not yet live)
        assert_eq!(m.push(AudioSource::Microphone, &[0.1; 1600], 16_000, 1).len(), 1600);
        // System arrives: mic is live but empty, so nothing is ready yet
        assert!(m.push(AudioSource::System, &[0.2; 9600], 48_000, 2).is_empty());
        // Next mic buffer aligns with queued system audio and is summed
        let out = m.push(AudioSource::Microphone, &[0.1; 1600], 16_000, 1);
        assert_eq!(out.len(), 1600);
        assert!((out[0] - 0.3).abs() < 1e-4);
    }

    #[test]
    fn stereo_48k_downmixes_and_resamples() {
        let mut r = Resampler::default();
        let out = r.process(&downmix(&[0.5; 9600], 2), 48_000); // 0.1s stereo
        assert!((out.len() as i64 - 1600).abs() <= 1);
        assert!((out[10] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn resampler_has_no_drift_across_odd_buffers() {
        // 512-frame 48 kHz buffers don't divide by 3; one hour of them must
        // still yield one hour of 16 kHz audio (per-buffer rounding lost 0.4%)
        let mut r = Resampler::default();
        let buf = vec![0.1f32; 512];
        let n_bufs = 48_000 * 3600 / 512;
        let total: usize = (0..n_bufs).map(|_| r.process(&buf, 48_000).len()).sum();
        let expected = n_bufs * 512 / 3;
        assert!((total as i64 - expected as i64).abs() <= 2, "{} vs {}", total, expected);
    }

    #[test]
    fn resampler_is_continuous_across_buffers() {
        // A ramp split across buffers must come out monotonic (no clicks)
        let mut r = Resampler::default();
        let ramp: Vec<f32> = (0..4410).map(|i| i as f32 / 4410.0).collect();
        let mut out = Vec::new();
        for chunk in ramp.chunks(441) {
            out.extend(r.process(chunk, 44_100));
        }
        assert!(out.windows(2).all(|w| w[1] >= w[0]));
    }

    #[test]
    fn soft_clip_bounds_output() {
        assert!(soft_clip(3.0) <= 1.0);
        assert_eq!(soft_clip(0.5), 0.5);
    }
}
