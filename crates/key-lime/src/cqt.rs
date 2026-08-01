//! Constant-Q Transform (CQT) chroma extraction.
//!
//! Unlike the Goertzel approach (fixed window for all frequencies), CQT uses
//! frequency-dependent window lengths: longer for low notes, shorter for high.
//! This gives uniform semitone resolution across the full pitch range.
//!
//! For key estimation, we only need chroma (12 pitch classes), so we compute
//! a "direct CQT" using Goertzel-like filters but with per-note window sizes.

#![allow(dead_code)]

use std::f32::consts::PI;

/// Precomputed CQT kernel parameters for one note.
#[derive(Clone)]
struct CqtNote {
    /// MIDI note number
    midi: u8,
    /// Frequency in Hz
    freq: f32,
    /// Window length (samples) for this note
    window_len: usize,
    /// Which pitch class (0-11) this note maps to
    pitch_class: usize,
    /// Goertzel coefficient: 2*cos(2*pi*freq/sr * window_len_ratio)
    coeff: f32,
    /// Precomputed Hanning window for this note's window_len
    window: Vec<f32>,
}

/// A CQT kernel table for one sample rate.
pub struct CqtTable {
    notes: Vec<CqtNote>,
    /// Minimum window length across all notes (used as hop constraint)
    min_window: usize,
}

impl CqtTable {
    /// Build a CQT table spanning from `min_midi` to `max_midi` at the given sample rate.
    pub fn new(sample_rate: u32, min_midi: u8, max_midi: u8) -> Self {
        Self::with_max_window(sample_rate, min_midi, max_midi, sample_rate as usize)
    }

    fn with_max_window(sample_rate: u32, min_midi: u8, max_midi: u8, max_window: usize) -> Self {
        let sr = sample_rate as f32;
        let bins_per_octave = 12.0_f32;
        let q = 1.0 / (2.0_f32.powf(1.0 / bins_per_octave) - 1.0);

        let mut notes = Vec::new();
        let mut min_window = usize::MAX;

        for midi in min_midi..=max_midi {
            let freq = 440.0 * 2.0_f32.powf((midi as f32 - 69.0) / 12.0);

            if freq >= sr * 0.45 {
                continue;
            }

            let window_len = (q * sr / freq).ceil() as usize;
            let window_len = window_len.min(max_window);

            if window_len < 4 {
                continue;
            }

            min_window = min_window.min(window_len);

            let pitch_class = (midi % 12) as usize;
            let normalized_freq = freq / sr;
            let coeff = 2.0 * (2.0 * PI * normalized_freq).cos();

            let window: Vec<f32> = (0..window_len)
                .map(|i| {
                    let phase = 2.0 * PI * i as f32 / window_len as f32;
                    0.5 * (1.0 - phase.cos())
                })
                .collect();

            notes.push(CqtNote {
                midi,
                freq,
                window_len,
                pitch_class,
                coeff,
                window,
            });
        }

        Self {
            notes,
            min_window: min_window.max(1),
        }
    }

    /// Suggested hop size: half the shortest note's window.
    pub fn suggested_hop(&self) -> usize {
        self.min_window / 2
    }

    /// Compute chroma from a segment of audio using the CQT.
    /// Each note uses its own window length, centered at `center` in the audio.
    /// Returns energy per pitch class [0..12].
    pub fn chroma_at(&self, samples: &[f32], center: usize) -> [f32; 12] {
        let mut chroma = [0.0f32; 12];

        for note in &self.notes {
            let half = note.window_len / 2;
            let frame_start = if center >= half { center - half } else { 0 };
            let frame_end = (frame_start + note.window_len).min(samples.len());
            let actual_len = frame_end - frame_start;

            if actual_len < note.window_len / 2 {
                continue;
            }

            let mut s1: f32 = 0.0;
            let mut s2: f32 = 0.0;

            for i in 0..actual_len {
                let x = samples[frame_start + i] * note.window[i.min(note.window.len() - 1)];
                let s0 = x + note.coeff * s1 - s2;
                s2 = s1;
                s1 = s0;
            }

            let power = s1 * s1 + s2 * s2 - note.coeff * s1 * s2;
            let normalized_power = power / (actual_len as f32 * actual_len as f32);
            chroma[note.pitch_class] += normalized_power.max(0.0);
        }

        chroma
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cqt_table_builds() {
        let table = CqtTable::new(44_100, 36, 96);
        assert!(!table.notes.is_empty());
        assert!(table.suggested_hop() > 0);
    }

    #[test]
    fn cqt_detects_440hz() {
        let sr = 44_100u32;
        let freq = 440.0f32; // A4 = MIDI 69, pitch class 9
        let samples: Vec<f32> = (0..sr as usize * 2)
            .map(|i| (2.0 * PI * freq * i as f32 / sr as f32).sin())
            .collect();

        let table = CqtTable::new(sr, 36, 96);
        let chroma = table.chroma_at(&samples, sr as usize);

        let max_pc = chroma
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, _)| i)
            .unwrap();
        assert_eq!(max_pc, 9, "expected A (pc=9), got pc={max_pc}");
    }

    #[test]
    fn cqt_silence_is_zero() {
        let samples = vec![0.0f32; 44_100];
        let table = CqtTable::new(44_100, 36, 96);
        let chroma = table.chroma_at(&samples, 22_050);
        let total: f32 = chroma.iter().sum();
        assert!(total < 1e-10, "silence should produce zero chroma");
    }
}
