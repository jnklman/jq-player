//! Values the editor never stores but a show file edited by hand can hold:
//! negative times, a pan out of range, an output the show does not have.
//! Playback copes with each on its own; these warnings say what it does.

use jq_i18n::t;

use crate::model::{AdvanceAt, Cue, CueKind, RATE_MAX, RATE_MIN, SecondTrigger, Show, TRANSPOSE_MAX};

/// What is wrong with the cue's values, as messages in the current language.
pub fn check_values(cue: &Cue, show: &Show) -> Vec<String> {
    let mut issues = Vec::new();
    let mut negative = |value: f64, name: String| {
        if value < 0.0 {
            issues.push(t!("check.negative", name = name));
        }
    };
    negative(cue.pre_wait, t!("check.field.pre_wait"));
    if let AdvanceAt::After { secs } | AdvanceAt::BeforeEnd { secs } = cue.advance.at {
        negative(secs, t!("check.field.follow_time"));
    }
    let output = match &cue.kind {
        CueKind::Sound(s) => {
            negative(s.start, t!("check.field.start"));
            negative(s.fade_in, t!("check.field.fade_in"));
            negative(s.fade_out, t!("check.field.fade_out"));
            speed_issues(s.speed.rate, s.speed.transpose, &mut issues);
            Some((&s.output, Some(s.pan)))
        }
        CueKind::Playlist(p) => {
            negative(p.fade_in, t!("check.field.fade_in"));
            negative(p.fade_out, t!("check.field.fade_out"));
            negative(p.gap, t!("check.field.gap"));
            speed_issues(p.speed.rate, p.speed.transpose, &mut issues);
            // Overridden values of single items, by number.
            for (i, o) in p.items.iter().map(|item| &item.overrides).enumerate() {
                let mut item = Vec::new();
                let mut negative = |value: Option<f64>, name: String| {
                    if value.is_some_and(|v| v < 0.0) {
                        item.push(t!("check.negative", name = name));
                    }
                };
                negative(Some(o.start), t!("check.field.start"));
                negative(o.fade_in, t!("check.field.fade_in"));
                negative(o.fade_out, t!("check.field.fade_out"));
                speed_issues(o.rate.unwrap_or(100.0), o.transpose.unwrap_or(0.0), &mut item);
                if o.pan.is_some_and(|pan| !(-1.0..=1.0).contains(&pan)) {
                    item.push(t!("check.pan_range"));
                }
                issues.extend(item.into_iter().map(|issue| t!("check.item", number = i + 1, issue = issue)));
            }
            Some((&p.output, Some(p.pan)))
        }
        CueKind::Rate(r) => {
            negative(r.duration, t!("check.field.duration"));
            speed_issues(r.rate.unwrap_or(100.0), r.transpose.unwrap_or(0.0), &mut issues);
            None
        }
        CueKind::Volume(f) => {
            negative(f.duration.unwrap_or(0.0), t!("check.field.fade_duration"));
            None
        }
        CueKind::Pan(p) => {
            negative(p.duration, t!("check.field.duration"));
            if !(-1.0..=1.0).contains(&p.pan) {
                issues.push(t!("check.pan_range"));
            }
            None
        }
        CueKind::Reset(r) => {
            negative(r.duration, t!("check.field.duration"));
            None
        }
        CueKind::StartStop(c) => {
            negative(c.from.unwrap_or(0.0), t!("check.field.start_from"));
            None
        }
        CueKind::Position(p) => {
            negative(p.crossfade_ms, t!("check.field.crossfade"));
            None
        }
        CueKind::Wait(w) => {
            negative(w.duration, t!("check.field.wait"));
            None
        }
        _ => None,
    };
    if let Some((output, pan)) = output {
        if pan.is_some_and(|pan| !(-1.0..=1.0).contains(&pan)) {
            issues.push(t!("check.pan_range"));
        }
        if !show.outputs.iter().any(|o| &o.id == output) {
            issues.push(t!("check.output_missing", output = output));
        }
    }
    if cue.second_trigger == SecondTrigger::NextItem && !matches!(cue.kind, CueKind::Playlist(_)) {
        issues.push(t!("check.next_item_only_playlist"));
    }
    issues
}

fn speed_issues(rate: f64, transpose: f64, issues: &mut Vec<String>) {
    if !(RATE_MIN..=RATE_MAX).contains(&rate) {
        issues.push(t!("check.rate_range", min = RATE_MIN, max = RATE_MAX));
    }
    if !(-TRANSPOSE_MAX..=TRANSPOSE_MAX).contains(&transpose) {
        issues.push(t!("check.transpose_range", max = TRANSPOSE_MAX));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SoundCue;

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
        assert_eq!(
            issues,
            [
                "Item 2: Start is negative – counts as 0",
                "Item 2: Fade-in is negative – counts as 0",
                "Item 2: Rate is outside 50–150 % – will be limited",
                "Item 2: Pan is outside −1…+1 – will be limited",
            ]
        );
    }

    #[test]
    fn next_item_on_a_sound_cue_is_reported() {
        let show = Show { outputs: Show::default().outputs, ..Show::default() };
        let mut cue = Cue::new(CueKind::Sound(SoundCue::new("a.wav")));
        cue.second_trigger = SecondTrigger::NextItem;
        assert_eq!(
            check_values(&cue, &show),
            ["“Next item” on a second trigger only exists for playlists – ignored"]
        );
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
                "Pre-wait is negative – counts as 0",
                "Start is negative – counts as 0",
                "Fade-in is negative – counts as 0",
                "Rate is outside 50–150 % – will be limited",
                "Transposition is outside ±12 semitones – will be limited",
                "Pan is outside −1…+1 – will be limited",
                "Output “gone” does not exist in the show – plays on channels 1/2",
            ]
        );
    }
}
