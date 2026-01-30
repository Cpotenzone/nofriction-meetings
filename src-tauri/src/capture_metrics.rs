// noFriction Meetings - Capture Metrics
// Per-meeting statistics for monitoring deduplication effectiveness
//
// Tracks:
// - frames_in: Total frames received from capture engine
// - states_out: Number of states created
// - images_written: Number of keyframe images saved to disk
// - ocr_calls: Number of OCR invocations (state boundaries only)
// - bytes_saved: Estimated disk savings from deduplication
// - cpu_time: Accumulated processing time

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Metrics for a single meeting session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingMetrics {
    pub meeting_id: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,

    // Frame processing
    pub frames_in: u64,
    pub states_out: u64,
    pub images_written: u64,
    pub duplicates_skipped: u64,

    // Text extraction
    pub ocr_calls: u64,
    pub snapshots_created: u64,
    pub patches_created: u64,

    // Resource usage
    pub bytes_saved_estimate: u64,
    pub cpu_time_ms: u64,

    // Derived metrics
    pub dedup_ratio: f32,
    pub avg_state_duration_ms: f64,
}

impl MeetingMetrics {
    /// Create new metrics for a meeting
    pub fn new(meeting_id: &str) -> Self {
        Self {
            meeting_id: meeting_id.to_string(),
            started_at: Utc::now(),
            ended_at: None,
            frames_in: 0,
            states_out: 0,
            images_written: 0,
            duplicates_skipped: 0,
            ocr_calls: 0,
            snapshots_created: 0,
            patches_created: 0,
            bytes_saved_estimate: 0,
            cpu_time_ms: 0,
            dedup_ratio: 0.0,
            avg_state_duration_ms: 0.0,
        }
    }

    /// Finalize metrics at meeting end
    pub fn finalize(&mut self) {
        self.ended_at = Some(Utc::now());

        // Calculate dedup ratio
        if self.frames_in > 0 {
            self.dedup_ratio = 1.0 - (self.states_out as f32 / self.frames_in as f32);
        }

        // Calculate average state duration
        if self.states_out > 0 {
            if let Some(ended) = self.ended_at {
                let total_ms = (ended - self.started_at).num_milliseconds() as f64;
                self.avg_state_duration_ms = total_ms / self.states_out as f64;
            }
        }
    }

    /// Log metrics summary
    pub fn log_summary(&self) {
        log::info!("══════════════════════════════════════════════════════════");
        log::info!("📊 Meeting Metrics: {}", self.meeting_id);
        log::info!("══════════════════════════════════════════════════════════");
        log::info!("  Frames received:    {:>8}", self.frames_in);
        log::info!("  States created:     {:>8}", self.states_out);
        log::info!("  Images written:     {:>8}", self.images_written);
        log::info!("  Duplicates skipped: {:>8}", self.duplicates_skipped);
        log::info!("  Dedup ratio:        {:>7.1}%", self.dedup_ratio * 100.0);
        log::info!("──────────────────────────────────────────────────────────");
        log::info!("  OCR calls:          {:>8}", self.ocr_calls);
        log::info!("  Snapshots:          {:>8}", self.snapshots_created);
        log::info!("  Patches:            {:>8}", self.patches_created);
        log::info!("──────────────────────────────────────────────────────────");
        log::info!(
            "  Bytes saved (est):  {:>8} KB",
            self.bytes_saved_estimate / 1024
        );
        log::info!("  CPU time:           {:>8} ms", self.cpu_time_ms);
        log::info!(
            "  Avg state duration: {:>8.0} ms",
            self.avg_state_duration_ms
        );
        log::info!("══════════════════════════════════════════════════════════");
    }
}

/// Live metrics collector (thread-safe)
pub struct MetricsCollector {
    meeting_id: Mutex<Option<String>>,
    frames_in: AtomicU64,
    states_out: AtomicU64,
    images_written: AtomicU64,
    duplicates_skipped: AtomicU64,
    ocr_calls: AtomicU64,
    snapshots_created: AtomicU64,
    patches_created: AtomicU64,
    bytes_saved: AtomicU64,
    cpu_time_ns: AtomicU64,
    started_at: Mutex<Option<DateTime<Utc>>>,
}

impl MetricsCollector {
    /// Create a new collector
    pub fn new() -> Self {
        Self {
            meeting_id: Mutex::new(None),
            frames_in: AtomicU64::new(0),
            states_out: AtomicU64::new(0),
            images_written: AtomicU64::new(0),
            duplicates_skipped: AtomicU64::new(0),
            ocr_calls: AtomicU64::new(0),
            snapshots_created: AtomicU64::new(0),
            patches_created: AtomicU64::new(0),
            bytes_saved: AtomicU64::new(0),
            cpu_time_ns: AtomicU64::new(0),
            started_at: Mutex::new(None),
        }
    }

    /// Start collecting for a new meeting
    pub fn start_meeting(&self, meeting_id: &str) {
        // Reset counters
        self.frames_in.store(0, Ordering::SeqCst);
        self.states_out.store(0, Ordering::SeqCst);
        self.images_written.store(0, Ordering::SeqCst);
        self.duplicates_skipped.store(0, Ordering::SeqCst);
        self.ocr_calls.store(0, Ordering::SeqCst);
        self.snapshots_created.store(0, Ordering::SeqCst);
        self.patches_created.store(0, Ordering::SeqCst);
        self.bytes_saved.store(0, Ordering::SeqCst);
        self.cpu_time_ns.store(0, Ordering::SeqCst);

        *self.meeting_id.lock() = Some(meeting_id.to_string());
        *self.started_at.lock() = Some(Utc::now());

        log::info!("📊 Metrics collector started for meeting: {}", meeting_id);
    }

    /// End collection and return final metrics
    pub fn end_meeting(&self) -> Option<MeetingMetrics> {
        let meeting_id = self.meeting_id.lock().take()?;
        let started_at = self.started_at.lock().take()?;

        let mut metrics = MeetingMetrics {
            meeting_id,
            started_at,
            ended_at: None,
            frames_in: self.frames_in.load(Ordering::SeqCst),
            states_out: self.states_out.load(Ordering::SeqCst),
            images_written: self.images_written.load(Ordering::SeqCst),
            duplicates_skipped: self.duplicates_skipped.load(Ordering::SeqCst),
            ocr_calls: self.ocr_calls.load(Ordering::SeqCst),
            snapshots_created: self.snapshots_created.load(Ordering::SeqCst),
            patches_created: self.patches_created.load(Ordering::SeqCst),
            bytes_saved_estimate: self.bytes_saved.load(Ordering::SeqCst),
            cpu_time_ms: self.cpu_time_ns.load(Ordering::SeqCst) / 1_000_000,
            dedup_ratio: 0.0,
            avg_state_duration_ms: 0.0,
        };

        metrics.finalize();
        Some(metrics)
    }

    /// Record a frame being processed
    pub fn record_frame(&self) {
        self.frames_in.fetch_add(1, Ordering::SeqCst);
    }

    /// Record a new state being created
    pub fn record_new_state(&self) {
        self.states_out.fetch_add(1, Ordering::SeqCst);
    }

    /// Record an image being written to disk
    pub fn record_image_write(&self, bytes: u64) {
        self.images_written.fetch_add(1, Ordering::SeqCst);
    }

    /// Record a duplicate frame being skipped
    pub fn record_duplicate_skipped(&self, estimated_bytes: u64) {
        self.duplicates_skipped.fetch_add(1, Ordering::SeqCst);
        self.bytes_saved
            .fetch_add(estimated_bytes, Ordering::SeqCst);
    }

    /// Record an OCR call
    pub fn record_ocr_call(&self) {
        self.ocr_calls.fetch_add(1, Ordering::SeqCst);
    }

    /// Record a text snapshot
    pub fn record_snapshot(&self) {
        self.snapshots_created.fetch_add(1, Ordering::SeqCst);
    }

    /// Record a text patch
    pub fn record_patch(&self) {
        self.patches_created.fetch_add(1, Ordering::SeqCst);
    }

    /// Record CPU time for processing
    pub fn record_cpu_time(&self, duration: Duration) {
        self.cpu_time_ns
            .fetch_add(duration.as_nanos() as u64, Ordering::SeqCst);
    }

    /// Create a timer that automatically records CPU time
    pub fn start_timer(&self) -> MetricsTimer {
        MetricsTimer {
            start: Instant::now(),
            collector: self,
        }
    }

    /// Get current metrics snapshot (for monitoring)
    pub fn snapshot(&self) -> Option<MeetingMetrics> {
        let meeting_id = self.meeting_id.lock().clone()?;
        let started_at = self.started_at.lock().clone()?;

        Some(MeetingMetrics {
            meeting_id,
            started_at,
            ended_at: None,
            frames_in: self.frames_in.load(Ordering::SeqCst),
            states_out: self.states_out.load(Ordering::SeqCst),
            images_written: self.images_written.load(Ordering::SeqCst),
            duplicates_skipped: self.duplicates_skipped.load(Ordering::SeqCst),
            ocr_calls: self.ocr_calls.load(Ordering::SeqCst),
            snapshots_created: self.snapshots_created.load(Ordering::SeqCst),
            patches_created: self.patches_created.load(Ordering::SeqCst),
            bytes_saved_estimate: self.bytes_saved.load(Ordering::SeqCst),
            cpu_time_ms: self.cpu_time_ns.load(Ordering::SeqCst) / 1_000_000,
            dedup_ratio: 0.0,
            avg_state_duration_ms: 0.0,
        })
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Timer that automatically records elapsed time when dropped
pub struct MetricsTimer<'a> {
    start: Instant,
    collector: &'a MetricsCollector,
}

impl<'a> Drop for MetricsTimer<'a> {
    fn drop(&mut self) {
        self.collector.record_cpu_time(self.start.elapsed());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collection() {
        let collector = MetricsCollector::new();

        collector.start_meeting("test_meeting");

        // Simulate some activity
        for _ in 0..100 {
            collector.record_frame();
        }

        for _ in 0..5 {
            collector.record_new_state();
        }

        collector.record_duplicate_skipped(50_000);
        collector.record_duplicate_skipped(50_000);

        let metrics = collector.end_meeting().unwrap();

        assert_eq!(metrics.frames_in, 100);
        assert_eq!(metrics.states_out, 5);
        assert_eq!(metrics.duplicates_skipped, 2);
        assert_eq!(metrics.bytes_saved_estimate, 100_000);
        assert!(metrics.dedup_ratio > 0.9); // 95% dedup
    }
}
