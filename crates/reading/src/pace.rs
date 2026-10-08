//! Minutes to read, at 230 words a minute until the reader's own pace is
//! measured from issues they finished, Kindle-style. Each measurement is
//! clamped so a tab left open overnight can't skew it.

pub const DEFAULT_WPM: u32 = 230;
const MIN_WPM: u32 = 120;
const MAX_WPM: u32 = 600;
/// A finished item shorter than this says little about pace.
const MIN_SAMPLE_WORDS: u32 = 150;
/// Finished items needed before the pace is the reader's own.
const MIN_SAMPLES: usize = 3;

/// Whole minutes for `words`, never under one.
pub fn minutes(words: u32, wpm: u32) -> u32 {
    let wpm = wpm.max(1);
    words.div_ceil(wpm).max(1)
}

/// Minutes left after reading `progress` (0 to 1) of `words`.
pub fn minutes_left(words: u32, progress: f64, wpm: u32) -> u32 {
    let left = (f64::from(words) * (1.0 - progress.clamp(0.0, 1.0))).round();
    if left <= 0.0 {
        return 0;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "left is a rounded word count between 0 and u32::MAX"
    )]
    let left = left as u32;
    minutes(left, wpm)
}

/// The reader's words per minute from finished items, as `(words,
/// dwell_ms)`, or `None` until there are enough of them. The median of
/// clamped samples.
pub fn reader_wpm(finished: &[(u32, u64)]) -> Option<u32> {
    let mut samples: Vec<u32> = finished
        .iter()
        .filter(|(words, dwell)| *words >= MIN_SAMPLE_WORDS && *dwell > 0)
        .map(|(words, dwell)| {
            let wpm = u64::from(*words) * 60_000 / dwell;
            u32::try_from(wpm)
                .unwrap_or(u32::MAX)
                .clamp(MIN_WPM, MAX_WPM)
        })
        .collect();
    if samples.len() < MIN_SAMPLES {
        return None;
    }
    samples.sort_unstable();
    Some(samples[samples.len() / 2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minutes_round_up_and_never_show_zero() {
        assert_eq!(minutes(0, DEFAULT_WPM), 1);
        assert_eq!(minutes(230, DEFAULT_WPM), 1);
        assert_eq!(minutes(231, DEFAULT_WPM), 2);
        assert_eq!(minutes(3220, DEFAULT_WPM), 14);
    }

    #[test]
    fn minutes_left_follow_progress() {
        assert_eq!(minutes_left(2300, 0.0, 230), 10);
        assert_eq!(minutes_left(2300, 0.62, 230), 4);
        assert_eq!(minutes_left(2300, 1.0, 230), 0);
    }

    #[test]
    fn the_readers_pace_needs_three_real_reads_and_ignores_an_overnight_tab() {
        assert_eq!(reader_wpm(&[(1000, 240_000), (1000, 240_000)]), None);
        // 250, 300 and an overnight tab (clamped to 120): median 250.
        let pace = reader_wpm(&[(1000, 240_000), (1500, 300_000), (1000, 36_000_000)]);
        assert_eq!(pace, Some(250));
        // Short items don't count.
        assert_eq!(
            reader_wpm(&[(100, 10_000), (100, 10_000), (100, 10_000)]),
            None
        );
    }
}
