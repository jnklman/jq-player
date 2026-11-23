//! Values the editor never stores but a show file edited by hand can hold:
//! negative times, a pan out of range, an output the show does not have.
//! Playback copes with each on its own; these warnings say what it does.

use serde::Serialize;

use crate::model::{AdvanceAt, Cue, CueKind, RATE_MAX, RATE_MIN, SecondTrigger, Show, TRANSPOSE_MAX};

/// A time field a hand-edited show file can hold a negative value in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeField {
    PreWait,
    FollowTime,
    Start,
    FadeIn,
    FadeOut,
    Gap,
    Duration,
    FadeDuration,
    StartFrom,
    Crossfade,
    Wait,
}

/// What is wrong with a cue's values (the UI words it). `item`: of a
/// playlist item's overrides, counted from 1.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValueIssue {
    /// Counts as 0.
    Negative { field: TimeField, item: Option<usize> },
    /// Will be limited to −1…+1.
    PanRange { item: Option<usize> },
    /// Will be limited to `min`–`max` %.
    RateRange { item: Option<usize>, min: f64, max: f64 },
    /// Will be limited to ±`max` semitones.
    TransposeRange { item: Option<usize>, max: f64 },
    /// The show has no such output: plays on channels 1/2.
    OutputUnknown { output: String },
    /// "Next item" on a second trigger of a cue that is no playlist.
    NextItemOnlyPlaylist,
}

/// What is wrong with the cue's values.
pub fn check_values(cue: &Cue, show: &Show) -> Vec<ValueIssue> {
    let mut issues = Vec::new();
    let mut negative = |value: f64, field: TimeField| {
        if value < 0.0 {
            issues.push(ValueIssue::Negative { field, item: None });
        }
    };
    negative(cue.pre_wait, TimeField::PreWait);
    if let AdvanceAt::After { secs } | AdvanceAt::BeforeEnd { secs } = cue.advance.at {
        negative(secs, TimeField::FollowTime);
    }
    let output = match &cue.kind {
        CueKind::Sound(s) => {
            negative(s.start, TimeField::Start);
            negative(s.fade_in, TimeField::FadeIn);
            negative(s.fade_out, TimeField::FadeOut);
            speed_issues(s.speed.rate, s.speed.transpose, None, &mut issues);
            Some((&s.output, Some(s.pan)))
        }
        CueKind::Playlist(p) => {
            negative(p.fade_in, TimeField::FadeIn);
            negative(p.fade_out, TimeField::FadeOut);
            negative(p.gap, TimeField::Gap);
            speed_issues(p.speed.rate, p.speed.transpose, None, &mut issues);
            // Overridden values of single items, by number.
            for (i, o) in p.items.iter().map(|item| &item.overrides).enumerate() {
                let item = Some(i + 1);
                let times = [
                    (Some(o.start), TimeField::Start),
                    (o.fade_in, TimeField::FadeIn),
                    (o.fade_out, TimeField::FadeOut),
                ];
                for (value, field) in times {
                    if value.is_some_and(|v| v < 0.0) {
                        issues.push(ValueIssue::Negative { field, item });
                    }
                }
                speed_issues(o.rate.unwrap_or(100.0), o.transpose.unwrap_or(0.0), item, &mut issues);
                if o.pan.is_some_and(|pan| !(-1.0..=1.0).contains(&pan)) {
                    issues.push(ValueIssue::PanRange { item });
                }
            }
            Some((&p.output, Some(p.pan)))
        }
        CueKind::Rate(r) => {
            negative(r.duration, TimeField::Duration);
            speed_issues(r.rate.unwrap_or(100.0), r.transpose.unwrap_or(0.0), None, &mut issues);
            None
        }
        CueKind::Volume(f) => {
            negative(f.duration.unwrap_or(0.0), TimeField::FadeDuration);
            None
        }
        CueKind::Pan(p) => {
            negative(p.duration, TimeField::Duration);
            if !(-1.0..=1.0).contains(&p.pan) {
                issues.push(ValueIssue::PanRange { item: None });
            }
            None
        }
        CueKind::Reset(r) => {
            negative(r.duration, TimeField::Duration);
            None
        }
        CueKind::StartStop(c) => {
            negative(c.from.unwrap_or(0.0), TimeField::StartFrom);
            None
        }
        CueKind::Position(p) => {
            negative(p.crossfade_ms, TimeField::Crossfade);
            None
        }
        CueKind::Wait(w) => {
            negative(w.duration, TimeField::Wait);
            None
        }
        _ => None,
    };
    if let Some((output, pan)) = output {
        if pan.is_some_and(|pan| !(-1.0..=1.0).contains(&pan)) {
            issues.push(ValueIssue::PanRange { item: None });
        }
        if !show.outputs.iter().any(|o| &o.id == output) {
            issues.push(ValueIssue::OutputUnknown { output: output.clone() });
        }
    }
    if cue.second_trigger == SecondTrigger::NextItem && !matches!(cue.kind, CueKind::Playlist(_)) {
        issues.push(ValueIssue::NextItemOnlyPlaylist);
    }
    issues
}

fn speed_issues(rate: f64, transpose: f64, item: Option<usize>, issues: &mut Vec<ValueIssue>) {
    if !(RATE_MIN..=RATE_MAX).contains(&rate) {
        issues.push(ValueIssue::RateRange { item, min: RATE_MIN, max: RATE_MAX });
    }
    if !(-TRANSPOSE_MAX..=TRANSPOSE_MAX).contains(&transpose) {
        issues.push(ValueIssue::TransposeRange { item, max: TRANSPOSE_MAX });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SoundCue;
    use ValueIssue::*;

    const RATE: ValueIssue = RateRange { item: None, min: RATE_MIN, max: RATE_MAX };
    const TRANSPOSE: ValueIssue = TransposeRange { item: None, max: TRANSPOSE_MAX };
    const PAN: ValueIssue = PanRange { item: None };

    fn negative(field: TimeField) -> ValueIssue {
        Negative { field, item: None }
    }

    #[test]
    fn hand_edited_item_overrides_are_reported_by_number() {
        let show = Show { outputs: Show::default().outputs, ..Show::default() };
        let mut p = crate::model::PlaylistCue::new(vec!["a.wav".into(), "b.wav".into()]);
        let o = crate::model::ItemOverrides {
            start: -2.0,
            fade_in: Some(-1.0),
            rate: Some(10.0),
            pan: Some(2.0),
            ..Default::default()
        };
        p.items[1].overrides = o;
        let issues = check_values(&Cue::new(CueKind::Playlist(p)), &show);
        let item = Some(2);
        assert_eq!(
            issues,
            [
                Negative { field: TimeField::Start, item },
                Negative { field: TimeField::FadeIn, item },
                RateRange { item, min: RATE_MIN, max: RATE_MAX },
                PanRange { item },
            ]
        );
    }

    #[test]
    fn next_item_on_a_sound_cue_is_reported() {
        let show = Show { outputs: Show::default().outputs, ..Show::default() };
        let mut cue = Cue::new(CueKind::Sound(SoundCue::new("a.wav")));
        cue.second_trigger = SecondTrigger::NextItem;
        assert_eq!(check_values(&cue, &show), [NextItemOnlyPlaylist]);
        let mut list = Cue::new(CueKind::Playlist(crate::model::PlaylistCue::new(vec!["a.wav".into()])));
        list.second_trigger = SecondTrigger::NextItem;
        assert!(check_values(&list, &show).is_empty());
    }

    #[test]
    fn hand_edited_values_are_reported() {
        let show = Show::default();
        let fine = Cue::new(CueKind::Sound(SoundCue::new("a.wav")));
        assert!(check_values(&fine, &show).is_empty());

        let mut s = SoundCue { start: -5.0, fade_in: -2.0, pan: -3.0, ..SoundCue::new("a.wav") };
        s.speed.rate = 300.0;
        s.speed.transpose = -13.0;
        s.output = "gone".into();
        let mut cue = Cue::new(CueKind::Sound(s));
        cue.pre_wait = -3.0;
        assert_eq!(
            check_values(&cue, &show),
            [
                negative(TimeField::PreWait),
                negative(TimeField::Start),
                negative(TimeField::FadeIn),
                RATE,
                TRANSPOSE,
                PAN,
                OutputUnknown { output: "gone".into() },
            ]
        );
    }

    #[test]
    fn hand_edited_control_cues_are_reported() {
        use crate::model::{ControlKind, PreviousKeyword, Targets, WaitCue};
        let show = Show::default();
        let control = |kind, edit: fn(&mut CueKind)| {
            let mut kind = CueKind::control(kind, Targets::Previous(PreviousKeyword::Previous));
            edit(&mut kind);
            check_values(&Cue::new(kind), &show)
        };

        assert!(control(ControlKind::Rate, |_| {}).is_empty(), "new cues are fine");
        assert_eq!(
            control(ControlKind::Rate, |k| if let CueKind::Rate(r) = k {
                (r.duration, r.rate, r.transpose) = (-1.0, Some(200.0), Some(20.0));
            }),
            [negative(TimeField::Duration), RATE, TRANSPOSE]
        );
        assert_eq!(
            control(ControlKind::Volume, |k| if let CueKind::Volume(v) = k {
                v.duration = Some(-2.0);
            }),
            [negative(TimeField::FadeDuration)]
        );
        assert_eq!(
            control(ControlKind::Pan, |k| if let CueKind::Pan(p) = k {
                (p.duration, p.pan) = (-1.0, 1.5);
            }),
            [negative(TimeField::Duration), PAN]
        );
        assert_eq!(
            control(ControlKind::Reset, |k| if let CueKind::Reset(r) = k {
                r.duration = -1.0;
            }),
            [negative(TimeField::Duration)]
        );
        assert_eq!(
            control(ControlKind::StartStop, |k| if let CueKind::StartStop(s) = k {
                s.from = Some(-4.0);
            }),
            [negative(TimeField::StartFrom)]
        );
        assert_eq!(
            control(ControlKind::Position, |k| if let CueKind::Position(p) = k {
                p.crossfade_ms = -10.0;
            }),
            [negative(TimeField::Crossfade)]
        );
        let mut wait = Cue::new(CueKind::Wait(WaitCue { duration: -1.0 }));
        wait.advance = crate::model::Advance::new(AdvanceAt::After { secs: -2.0 });
        assert_eq!(check_values(&wait, &show), [negative(TimeField::FollowTime), negative(TimeField::Wait)]);
    }

    #[test]
    fn hand_edited_playlists_are_reported() {
        let show = Show::default();
        let mut p = crate::model::PlaylistCue::new(vec!["a.wav".into()]);
        (p.fade_out, p.gap, p.pan, p.output) = (-1.0, -2.0, -1.5, "gone".into());
        p.speed.transpose = 13.0;
        assert_eq!(
            check_values(&Cue::new(CueKind::Playlist(p)), &show),
            [
                negative(TimeField::FadeOut),
                negative(TimeField::Gap),
                TRANSPOSE,
                PAN,
                OutputUnknown { output: "gone".into() },
            ]
        );
    }
}
