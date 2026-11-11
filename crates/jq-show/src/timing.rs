//! Positions and fade times of a sound cue checked against the length of its
//! file, e.g. after the file was replaced by a shorter one. Playback clamps
//! such values on its own; these warnings say what will sound different.

use std::fmt;

use jq_i18n::t;
use serde::Serialize;

use crate::model::SoundCue;

/// Seconds a position may exceed the file without a warning: header lengths
/// and decoded lengths differ by a few samples.
const TOLERANCE: f64 = 0.001;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TimingIssue {
    /// Nothing plays: the start lies at or behind the end (or file end).
    StartPastEnd,
    /// Playback stops at the end of the file instead.
    EndPastFile,
    /// The vamp's end is not behind its start; it is skipped.
    VampEmpty { index: usize },
    /// The vamp ends before the start and is never reached.
    VampBeforeStart { index: usize },
    /// The vamp starts behind the end and is never reached.
    VampPastEnd { index: usize },
    /// The vamp overlaps an earlier one (by start) and is skipped.
    VampOverlap { index: usize, other: usize },
    /// The vamp ends behind the end and loops back from there instead.
    VampCut { index: usize },
    /// The mark lies outside start–end and is never reached.
    MarkOutside { index: usize },
    /// The mark is there twice (one is enough).
    MarkDuplicate { index: usize },
    /// The fade-in is longer than the start–end range.
    FadeInTooLong,
    /// The end fade is longer than the start–end range.
    EndFadeTooLong,
}

impl fmt::Display for TimingIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StartPastEnd => f.write_str(&t!("timing.start_past_end")),
            Self::EndPastFile => f.write_str(&t!("timing.end_past_file")),
            Self::VampEmpty { index } => f.write_str(&t!("timing.vamp_empty", number = index + 1)),
            Self::VampBeforeStart { index } => {
                f.write_str(&t!("timing.vamp_before_start", number = index + 1))
            }
            Self::VampOverlap { index, other } => {
                f.write_str(&t!("timing.vamp_overlap", number = index + 1, other = other + 1))
            }
            Self::VampPastEnd { index } => f.write_str(&t!("timing.vamp_past_end", number = index + 1)),
            Self::VampCut { index } => f.write_str(&t!("timing.vamp_cut", number = index + 1)),
            Self::MarkOutside { index } => f.write_str(&t!("timing.mark_outside", number = index + 1)),
            Self::MarkDuplicate { index } => f.write_str(&t!("timing.mark_duplicate", number = index + 1)),
            Self::FadeInTooLong => f.write_str(&t!("timing.fade_in_too_long")),
            Self::EndFadeTooLong => f.write_str(&t!("timing.end_fade_too_long")),
        }
    }
}

/// Checks `s` against its file's length in seconds.
pub fn check_timing(s: &SoundCue, file_secs: f64) -> Vec<TimingIssue> {
    let mut issues = Vec::new();
    if s.end.is_some_and(|e| e > file_secs + TOLERANCE) {
        issues.push(TimingIssue::EndPastFile);
    }
    let end = s.end.map_or(file_secs, |e| e.min(file_secs));
    if s.start >= end {
        issues.push(TimingIssue::StartPastEnd);
        return issues;
    }
    // The vamps playback keeps (see the runner's loop regions), by start.
    let mut kept: Vec<usize> = Vec::new();
    for (index, v) in s.vamps.iter().enumerate() {
        if v.end <= v.start {
            issues.push(TimingIssue::VampEmpty { index });
        } else if v.start >= end {
            issues.push(TimingIssue::VampPastEnd { index });
        } else if v.end <= s.start {
            issues.push(TimingIssue::VampBeforeStart { index });
        } else {
            if v.end > end + TOLERANCE {
                issues.push(TimingIssue::VampCut { index });
            }
            kept.push(index);
        }
    }
    kept.sort_by(|a, b| s.vamps[*a].start.total_cmp(&s.vamps[*b].start));
    let mut last: Option<usize> = None;
    for index in kept {
        match last {
            Some(other) if s.vamps[other].end.min(end) > s.vamps[index].start => {
                issues.push(TimingIssue::VampOverlap { index, other })
            }
            _ => last = Some(index),
        }
    }
    for (index, &mark) in s.marks.iter().enumerate() {
        if mark < s.start || mark > end + TOLERANCE {
            issues.push(TimingIssue::MarkOutside { index });
        } else if s.marks[..index].contains(&mark) {
            issues.push(TimingIssue::MarkDuplicate { index });
        }
    }
    // Fades last real time: at 50 % the range takes twice as long.
    let range = (end - s.start) / s.speed.factors().0;
    if s.fade_in > range + TOLERANCE {
        issues.push(TimingIssue::FadeInTooLong);
    }
    if s.end_fade && s.fade_out > range + TOLERANCE {
        issues.push(TimingIssue::EndFadeTooLong);
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Vamp;

    fn cue(start: f64, end: Option<f64>) -> SoundCue {
        SoundCue { start, end, ..SoundCue::new("a.wav") }
    }

    #[test]
    fn fitting_cue_has_no_issues() {
        let mut s = cue(1.0, Some(9.0));
        s.vamps = vec![Vamp { start: 2.0, end: 4.0 }];
        (s.fade_in, s.fade_out, s.end_fade) = (2.0, 8.0, true);
        assert!(check_timing(&s, 10.0).is_empty());
        // Within the tolerance: header and decoded length differ slightly.
        assert!(check_timing(&cue(0.0, Some(10.0005)), 10.0).is_empty());
    }

    #[test]
    fn shorter_file_is_reported() {
        assert_eq!(check_timing(&cue(0.0, Some(12.0)), 10.0), [TimingIssue::EndPastFile]);
        assert_eq!(check_timing(&cue(11.0, None), 10.0), [TimingIssue::StartPastEnd]);
        assert_eq!(
            check_timing(&cue(11.0, Some(12.0)), 10.0),
            [TimingIssue::EndPastFile, TimingIssue::StartPastEnd]
        );
        // A start behind a stored end is reported even if the file fits.
        assert_eq!(check_timing(&cue(5.0, Some(4.0)), 10.0), [TimingIssue::StartPastEnd]);
    }

    #[test]
    fn vamps_and_fades_against_the_range() {
        let mut s = cue(1.0, None);
        s.vamps =
            vec![Vamp { start: 2.0, end: 4.0 }, Vamp { start: 5.0, end: 7.0 }, Vamp { start: 8.0, end: 9.0 }];
        (s.fade_in, s.fade_out) = (6.0, 6.0);
        assert_eq!(
            check_timing(&s, 6.0),
            [
                TimingIssue::VampCut { index: 1 },
                TimingIssue::VampPastEnd { index: 2 },
                TimingIssue::FadeInTooLong
            ]
        );
        // The fade-out time only matters for the end fade.
        s.end_fade = true;
        assert!(check_timing(&s, 6.0).contains(&TimingIssue::EndFadeTooLong));
    }

    #[test]
    fn vamps_playback_skips_are_reported() {
        let mut s = cue(2.0, None);
        s.vamps = vec![
            Vamp { start: 5.0, end: 4.0 },
            Vamp { start: 3.0, end: 3.0 },
            Vamp { start: 0.5, end: 1.5 },
            Vamp { start: 6.0, end: 8.0 },
            Vamp { start: 4.0, end: 7.0 },
        ];
        assert_eq!(
            check_timing(&s, 10.0),
            [
                TimingIssue::VampEmpty { index: 0 },
                TimingIssue::VampEmpty { index: 1 },
                TimingIssue::VampBeforeStart { index: 2 },
                TimingIssue::VampOverlap { index: 3, other: 4 },
            ]
        );
    }

    #[test]
    fn marks_outside_or_twice_are_reported() {
        let mut s = cue(0.0, None);
        s.marks = vec![-1.0, 0.0, 2.0, 2.0, 100.0];
        assert_eq!(
            check_timing(&s, 10.0),
            [
                TimingIssue::MarkOutside { index: 0 },
                TimingIssue::MarkDuplicate { index: 3 },
                TimingIssue::MarkOutside { index: 4 },
            ]
        );
    }

    #[test]
    fn fades_are_checked_in_real_time() {
        let mut s = SoundCue { fade_in: 6.0, ..cue(0.0, Some(4.0)) };
        assert!(check_timing(&s, 10.0).contains(&TimingIssue::FadeInTooLong));
        // At 50 % the 4 s range lasts 8 s.
        s.speed.rate = 50.0;
        assert!(check_timing(&s, 10.0).is_empty());
    }
}
