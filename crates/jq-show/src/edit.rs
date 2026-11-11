//! Editing operations on the cue list with undo/redo. Shows are small, so
//! every edit snapshots the whole show.

use jq_i18n::t;

use crate::model::{Advance, AdvanceAt, Cue, CueId, CueKind, GroupCue, GroupMode, LinkId, Show, Targets};
use crate::number::new_numbers;

const UNDO_LIMIT: usize = 200;

pub struct Editor {
    show: Show,
    undo: Vec<Show>,
    redo: Vec<Show>,
    /// The show as last saved (or loaded); undoing back to it is not dirty.
    saved: Show,
    revision: u64,
}

impl Editor {
    pub fn new(show: Show) -> Self {
        Self { saved: show.clone(), show, undo: Vec::new(), redo: Vec::new(), revision: 0 }
    }

    pub fn show(&self) -> &Show {
        &self.show
    }

    pub fn into_show(self) -> Show {
        self.show
    }

    /// True if the show differs from the last [`Editor::mark_saved`].
    pub fn is_dirty(&self) -> bool {
        self.show != self.saved
    }

    /// Counts every change, undo and redo; tells whether an autosave is current.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The show, as saved and in the undo and redo history.
    pub fn history(&self) -> impl Iterator<Item = &Show> {
        [&self.show, &self.saved].into_iter().chain(&self.undo).chain(&self.redo)
    }

    /// Changes the show and its whole history alike, so undo never brings
    /// the old value back (e.g. the media folder after saving elsewhere).
    /// Not an edit: neither dirty nor a step of its own.
    pub fn change_history(&mut self, f: impl Fn(&mut Show)) {
        f(&mut self.show);
        f(&mut self.saved);
        self.undo.iter_mut().chain(&mut self.redo).for_each(f);
    }

    pub fn mark_saved(&mut self) {
        self.saved = self.show.clone();
    }

    /// Applies an arbitrary change as one undo step. Linked cues follow
    /// the one that changed ([`Show::sync_links`]).
    pub fn change<R>(&mut self, f: impl FnOnce(&mut Show) -> R) -> R {
        self.undo.push(self.show.clone());
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.revision += 1;
        let result = f(&mut self.show);
        self.show.sync_links(self.undo.last());
        result
    }

    pub fn undo(&mut self) -> bool {
        let Some(prev) = self.undo.pop() else { return false };
        self.redo.push(std::mem::replace(&mut self.show, prev));
        self.revision += 1;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else { return false };
        self.undo.push(std::mem::replace(&mut self.show, next));
        self.revision += 1;
        true
    }

    /// Merges the last change into the one before, so one undo takes back both.
    pub fn join_last(&mut self) {
        if self.undo.len() >= 2 {
            self.undo.pop();
        }
    }

    /// Takes back the last change without a redo step, e.g. the first half
    /// of a combined edit whose second half failed.
    pub fn revert(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.show = prev;
            self.revision += 1;
        }
    }

    /// Inserts `cue` at `index` (clamped), e.g. at the playhead.
    pub fn insert(&mut self, index: usize, cue: Cue) -> CueId {
        let id = cue.id;
        self.change(|s| s.cues.insert(index.min(s.cues.len()), cue));
        id
    }

    /// Inserts `cue` into group `group` at `index` (default: end). Returns
    /// `None` if there is no such group.
    pub fn insert_into(&mut self, group: CueId, index: Option<usize>, cue: Cue) -> Option<CueId> {
        let id = cue.id;
        if !matches!(self.show.cue(group).map(|c| &c.kind), Some(CueKind::Group(_))) {
            return None;
        }
        self.change(|s| {
            if let Some(Cue { kind: CueKind::Group(g), .. }) = s.cue_mut(group) {
                g.cues.insert(index.unwrap_or(g.cues.len()).min(g.cues.len()), cue);
            }
        });
        Some(id)
    }

    /// Replaces the cue with the same id, wherever it is; its link stays
    /// (see [`Editor::link`]) and linked cues follow.
    pub fn update(&mut self, cue: Cue) -> bool {
        self.update_all(vec![cue])
    }

    /// Replaces several cues (each wherever it is) as one undo step; changes
    /// nothing unless all of them exist.
    pub fn update_all(&mut self, cues: Vec<Cue>) -> bool {
        if cues.iter().any(|c| self.show.cue(c.id).is_none()) {
            return false;
        }
        self.change(|s| {
            for mut cue in cues {
                if let Some(c) = s.cue_mut(cue.id) {
                    cue.link = c.link;
                    *c = cue;
                }
            }
        });
        true
    }

    /// Links `ids` with `source`, which they take their shared part from.
    /// Cues already linked bring the cues linked with them along.
    pub fn link(&mut self, source: CueId, ids: &[CueId]) -> Result<(), String> {
        let show = &self.show;
        let template = show.cue(source).ok_or_else(|| t!("edit.cue_not_found"))?;
        let mut cues = vec![template];
        for id in ids.iter().filter(|id| **id != source) {
            cues.push(show.cue(*id).ok_or_else(|| t!("edit.cue_not_found"))?);
        }
        if cues.len() < 2 {
            return Err(t!("edit.link_needs_two"));
        }
        if cues.iter().any(|c| !c.can_link()) {
            return Err(t!("edit.link_not_groups"));
        }
        if cues.iter().any(|c| c.kind.type_name() != template.kind.type_name()) {
            return Err(t!("edit.link_same_type"));
        }
        let joined: Vec<LinkId> = cues.iter().filter_map(|c| c.link).collect();
        let link = template.link.unwrap_or_default();
        let members: Vec<CueId> = show
            .all()
            .into_iter()
            .filter(|c| cues.iter().any(|k| k.id == c.id) || c.link.is_some_and(|l| joined.contains(&l)))
            .map(|c| c.id)
            .collect();
        let template = template.clone();
        self.change(|s| {
            for id in members {
                if let Some(c) = s.cue_mut(id) {
                    c.link = Some(link);
                    c.copy_shared(&template);
                }
            }
        });
        Ok(())
    }

    /// Takes cues out of their links; they keep their settings.
    pub fn unlink(&mut self, ids: &[CueId]) {
        self.change(|s| {
            for id in ids {
                if let Some(c) = s.cue_mut(*id) {
                    c.link = None;
                }
            }
        });
    }

    /// Deletes cues (also inside groups, with what they contain); control
    /// cues and the loop exits advances wait for lose references to them. A go-to cue keeps its target, which then counts as missing.
    pub fn delete(&mut self, ids: &[CueId]) {
        let mut gone: Vec<CueId> = ids.to_vec();
        gone.extend(ids.iter().filter_map(|id| self.show.cue(*id)).flat_map(Cue::descendants));
        fn clean(cues: &mut Vec<Cue>, gone: &[CueId]) {
            cues.retain(|c| !gone.contains(&c.id));
            for c in cues {
                if let AdvanceAt::LoopExit { of } = &mut c.advance.at
                    && of.is_some_and(|id| gone.contains(&id))
                {
                    *of = None;
                }
                if let Some(Targets::Cues(t)) = c.kind.targets_mut() {
                    t.retain(|id| !gone.contains(id));
                }
                if let CueKind::Group(g) = &mut c.kind {
                    clean(&mut g.cues, gone);
                }
            }
        }
        self.change(|s| clean(&mut s.cues, &gone));
    }

    /// Duplicates a cue right after itself (also inside a group), with new
    /// ids and no hotkey or MIDI trigger (they would start them along with
    /// the original). A
    /// `linked` copy is an instance of the original (not for groups).
    pub fn duplicate(&mut self, id: CueId, linked: bool) -> Option<CueId> {
        let list = self.show.siblings(id)?;
        let index = list.iter().position(|c| c.id == id)?;
        let prefix = match self.show.parent(id) {
            Some(g) if !g.number.is_empty() => format!("{}.", g.number),
            _ => String::new(),
        };
        let taken = self.show.all().into_iter().map(|c| c.number.as_str()).collect();
        let mut copy = list[index].clone();
        copy.renew_ids();
        copy.hotkey = None;
        copy.midi_trigger = None;
        // The copy gets a number between the original and the next cue.
        if !copy.number.is_empty() {
            copy.number = new_numbers(list, &prefix, index + 1, 1, &taken).remove(0);
        }
        let link = (linked && copy.can_link()).then(|| list[index].link.unwrap_or_default());
        copy.link = link;
        let new = copy.id;
        self.change(|s| {
            if let Some(list) = s.siblings_mut(id) {
                list[index].link = link.or(list[index].link);
                list.insert(index + 1, copy);
            }
        });
        Some(new)
    }

    /// Moves cues as a block to `to` within their list (index among the cues
    /// that stay), for drag & drop. They keep their order in the list; ids
    /// that aren't siblings of the first one are left where they are.
    pub fn move_to(&mut self, ids: &[CueId], to: usize) -> bool {
        let Some(&first) = ids.first() else { return false };
        let Some(list) = self.show.siblings(first) else { return false };
        if !list.iter().any(|c| ids.contains(&c.id)) {
            return false;
        }
        self.change(|s| {
            if let Some(list) = s.siblings_mut(first) {
                let (moving, mut rest): (Vec<Cue>, Vec<Cue>) =
                    std::mem::take(list).into_iter().partition(|c| ids.contains(&c.id));
                let at = to.min(rest.len());
                rest.splice(at..at, moving);
                *list = rest;
            }
        });
        true
    }

    /// Dissolves groups: each one's cues take its place in its list, and it
    /// plays as before as far as possible. The group's advance and pre-wait
    /// go to its cues (an all-at-once group's cues chain at their start; the
    /// pre-waits stay exact while they don't decrease), its hotkey and MIDI
    /// trigger and passes to the first one (main list only). Control cues aiming at the
    /// group (also as the entry before them) aim at its cues, a go-to cue at
    /// the first. Ids that aren't groups are left alone. Returns the cues
    /// that came out, in list order.
    pub fn ungroup(&mut self, ids: &[CueId]) -> Vec<CueId> {
        let groups: Vec<CueId> = ids
            .iter()
            .copied()
            .filter(|id| matches!(self.show.cue(*id).map(|c| &c.kind), Some(CueKind::Group(_))))
            .collect();
        if groups.is_empty() {
            return Vec::new();
        }
        self.change(|s| {
            let freed: Vec<CueId> = groups.iter().flat_map(|id| ungroup(s, *id)).collect();
            // A group dissolved after the one around it came out of that one.
            s.all().into_iter().map(|c| c.id).filter(|id| freed.contains(id)).collect()
        })
    }

    /// Puts cues of the main list into a new group in sequence, at the place
    /// of the first, in list order; the reverse of [`Editor::ungroup`].
    /// Groups don't nest, so neither groups nor cues inside one are taken. It plays as before: the group continues like the
    /// last cue and takes the first one's hotkey, MIDI trigger and passes
    /// (the others' are dropped, inside a group they would do nothing).
    /// Control cues acting on the entry before them that would now hit
    /// another one keep their target by name. The group gets a number if any
    /// of the cues has one, a free one before the first.
    pub fn group(&mut self, ids: &[CueId]) -> Result<CueId, String> {
        if ids.is_empty() {
            return Err(t!("edit.nothing_selected"));
        }
        let list = &self.show.cues;
        if ids.iter().any(|id| !list.iter().any(|c| c.id == *id)) {
            return Err(t!("edit.group_main_only"));
        }
        let picked = || list.iter().filter(|c| ids.contains(&c.id));
        if picked().any(Cue::is_scene) {
            return Err(t!("edit.group_not_scenes"));
        }
        if picked().any(|c| matches!(c.kind, CueKind::Group(_))) {
            return Err(t!("edit.group_not_nested"));
        }
        let at = list.iter().take_while(|c| !ids.contains(&c.id)).count();
        let number = picked().any(|c| !c.number.is_empty()).then(|| {
            let taken = self.show.all().into_iter().map(|c| c.number.as_str()).collect();
            // Before the first cue, which keeps its number.
            new_numbers(list, "", at, 1, &taken).remove(0)
        });
        // What control cues acting on the entry before them hit now.
        let previous: Vec<(CueId, CueId)> = list
            .iter()
            .filter(|c| matches!(c.kind.targets(), Some(Targets::Previous(_))))
            .filter_map(|c| Some((c.id, self.show.previous_control_target_before(c.id)?)))
            .collect();

        let mut group = Cue::new(CueKind::Group(GroupCue { mode: GroupMode::Sequence, cues: Vec::new() }));
        group.title = t!("edit.group_title");
        group.number = number.unwrap_or_default();
        let id = group.id;
        self.change(|s| {
            let list = &mut s.cues;
            let (mut cues, rest): (Vec<Cue>, Vec<Cue>) =
                std::mem::take(list).into_iter().partition(|c| ids.contains(&c.id));
            *list = rest;
            if let Some(last) = cues.last() {
                group.advance = last.advance;
            }
            if let Some(first) = cues.first() {
                group.passes = first.passes;
                group.hotkey = first.hotkey.clone();
                group.midi_trigger = first.midi_trigger;
                group.release_trigger = first.release_trigger;
            }
            for c in &mut cues {
                c.hotkey = None;
                c.midi_trigger = None;
                c.passes = Default::default();
            }
            let CueKind::Group(g) = &mut group.kind else { return };
            g.cues = cues;
            list.insert(at, group);
            for (cue, target) in previous {
                if s.previous_control_target_before(cue) != Some(target)
                    && let Some(t) = s.cue_mut(cue).and_then(|c| c.kind.targets_mut())
                {
                    *t = Targets::Cues(vec![target]);
                }
            }
        });
        Ok(id)
    }

    /// Numbers all cues 1, 2, 3, …; a group's cues get the group's number as
    /// prefix (2.1, 2.2, …, nested 2.1.1). Scene markers get none.
    pub fn renumber(&mut self) {
        fn walk(cues: &mut [Cue], prefix: &str) {
            let mut n = 0;
            for c in cues {
                // Scene markers have no number.
                if c.is_scene() {
                    c.number.clear();
                    continue;
                }
                n += 1;
                c.number = format!("{prefix}{n}");
                if let CueKind::Group(g) = &mut c.kind {
                    walk(&mut g.cues, &format!("{}.", c.number));
                }
            }
        }
        self.change(|s| walk(&mut s.cues, ""));
    }
}

/// Dissolves group `id` (see [`Editor::ungroup`]); the ids of its cues.
fn ungroup(show: &mut Show, id: CueId) -> Vec<CueId> {
    let main_list = show.index_of(id).is_some();
    // Control cues acting on the entry before them, which is the group.
    let previous: Vec<CueId> = show
        .siblings(id)
        .into_iter()
        .flatten()
        .filter(|c| matches!(c.kind.targets(), Some(Targets::Previous(_))))
        .filter(|c| show.previous_control_target_before(c.id) == Some(id))
        .map(|c| c.id)
        .collect();
    let Some(list) = show.siblings_mut(id) else { return Vec::new() };
    let Some(at) = list.iter().position(|c| c.id == id && matches!(c.kind, CueKind::Group(_))) else {
        return Vec::new();
    };
    let group = list.remove(at);
    let CueKind::Group(g) = group.kind else { return Vec::new() };
    let mut cues = g.cues;
    match g.mode {
        GroupMode::All => {
            // Each cue started with the group after its own pre-wait; now
            // each starts with the one before, so pre-waits count from there.
            let mut fired = 0.0;
            for c in &mut cues {
                c.pre_wait = (group.pre_wait + c.pre_wait - fired).max(0.0);
                fired += c.pre_wait;
                c.advance = Advance::new(AdvanceAt::Start);
            }
        }
        GroupMode::Sequence => {
            if let Some(first) = cues.first_mut() {
                first.pre_wait += group.pre_wait;
            }
        }
    }
    if let Some(last) = cues.last_mut() {
        last.advance = group.advance;
    }
    if main_list && let Some(first) = cues.first_mut() {
        first.passes = group.passes;
        first.hotkey = group.hotkey;
        first.midi_trigger = group.midi_trigger;
        first.release_trigger = group.release_trigger;
    }
    let freed: Vec<CueId> = cues.iter().map(|c| c.id).collect();
    list.splice(at..at, cues);

    fn retarget(cues: &mut [Cue], group: CueId, freed: &[CueId], previous: &[CueId]) {
        for c in cues {
            if previous.contains(&c.id)
                && let Some(t) = c.kind.targets_mut()
            {
                *t = Targets::Cues(freed.to_vec());
            }
            if let Some(Targets::Cues(t)) = c.kind.targets_mut()
                && let Some(i) = t.iter().position(|t| *t == group)
            {
                t.splice(i..=i, freed.iter().copied());
                let mut seen = std::collections::HashSet::new();
                t.retain(|id| seen.insert(*id));
            }
            match &mut c.kind {
                CueKind::GoTo(goto) if goto.target == Some(group) => {
                    goto.target = freed.first().copied().or(goto.target);
                }
                CueKind::Group(g) => retarget(&mut g.cues, group, freed, previous),
                _ => {}
            }
        }
    }
    retarget(&mut show.cues, id, &freed, &previous);
    freed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Advance, ControlKind, DevampMode, ExitLoop, ExitLoopCue, FadeCurve, GoToCue, GroupCue, GroupMode,
        LinkId, PreviousKeyword, SoundCue, VolumeAction, VolumeCue,
    };

    fn sound(n: &str) -> Cue {
        let mut c = Cue::new(CueKind::Sound(SoundCue::new(format!("{n}.wav"))));
        c.title = n.into();
        c
    }

    fn titles(e: &Editor) -> Vec<&str> {
        e.show().cues.iter().map(|c| c.title.as_str()).collect()
    }

    #[test]
    fn history_changes_without_an_undo_step() {
        let mut e = Editor::new(Show::default());
        e.insert(0, sound("a"));
        e.insert(1, sound("b"));
        e.undo();
        let revision = e.revision();
        e.change_history(|s| s.media_root = "../Alt/media".into());
        assert_eq!((e.revision(), e.is_dirty()), (revision, true), "no step of its own");
        assert!(e.history().all(|s| s.media_root == "../Alt/media"));
        e.undo();
        assert_eq!(e.show().media_root, "../Alt/media");
        e.redo();
        e.redo();
        assert_eq!((titles(&e), e.show().media_root.as_str()), (vec!["a", "b"], "../Alt/media"));
    }

    #[test]
    fn joined_and_reverted_changes() {
        let mut e = Editor::new(Show::default());
        let a = e.insert(0, sound("a"));
        e.insert(1, sound("b"));
        e.delete(&[a]);
        e.insert(0, sound("c"));
        e.join_last();
        assert_eq!(titles(&e), ["c", "b"]);
        assert!(e.undo());
        assert_eq!(titles(&e), ["a", "b"], "one step");

        e.delete(&[a]);
        e.revert();
        assert_eq!(titles(&e), ["a", "b"]);
        assert!(!e.can_redo(), "no redo step");
        assert!(e.undo());
        assert_eq!(titles(&e), ["a"]);
    }

    #[test]
    fn move_several_cues_as_a_block() {
        let mut e = Editor::new(Show::default());
        let ids: Vec<_> =
            ["a", "b", "c", "d", "e"].iter().enumerate().map(|(i, n)| e.insert(i, sound(n))).collect();
        // Selection order doesn't matter, list order is kept.
        assert!(e.move_to(&[ids[3], ids[1]], 0));
        assert_eq!(titles(&e), ["b", "d", "a", "c", "e"]);
        assert!(e.move_to(&[ids[1], ids[3]], 3));
        assert_eq!(titles(&e), ["a", "c", "e", "b", "d"]);
        assert!(e.move_to(&[ids[0], ids[2]], 1));
        assert_eq!(titles(&e), ["e", "a", "c", "b", "d"]);
        assert!(e.undo());
        assert_eq!(titles(&e), ["a", "c", "e", "b", "d"]);
        assert!(!e.move_to(&[], 0));
    }

    #[test]
    fn insert_move_duplicate_undo_redo() {
        let mut e = Editor::new(Show::default());
        let a = e.insert(0, sound("a"));
        e.insert(1, sound("b"));
        e.insert(1, sound("c"));
        assert_eq!(titles(&e), ["a", "c", "b"]);
        e.move_to(&[a], 2);
        assert_eq!(titles(&e), ["c", "b", "a"]);
        let copy = e.duplicate(a, false).unwrap();
        assert_ne!(copy, a);
        assert_eq!(titles(&e), ["c", "b", "a", "a"]);

        assert!(e.undo());
        assert!(e.undo());
        assert_eq!(titles(&e), ["a", "c", "b"]);
        assert!(e.redo());
        assert_eq!(titles(&e), ["c", "b", "a"]);
        e.renumber();
        assert!(!e.redo(), "a new edit clears redo");
        let numbers: Vec<_> = e.show().cues.iter().map(|c| c.number.as_str()).collect();
        assert_eq!(numbers, ["1", "2", "3"]);
        assert!(e.is_dirty());
        assert_eq!(e.revision(), 9, "6 edits, 2 undos, 1 redo");
    }

    #[test]
    fn update_all_is_one_undo_step_and_all_or_nothing() {
        let mut e = Editor::new(Show::default());
        let a = e.insert(0, sound("a"));
        let b = e.insert(1, sound("b"));
        let rename = |e: &Editor, id: CueId, title: &str| {
            let mut c = e.show().cue(id).unwrap().clone();
            c.title = title.into();
            c
        };
        let changed = vec![rename(&e, a, "x"), rename(&e, b, "y")];
        assert!(e.update_all(changed));
        assert_eq!(titles(&e), ["x", "y"]);
        e.undo();
        assert_eq!(titles(&e), ["a", "b"]);

        let missing = vec![rename(&e, a, "x"), sound("gone")];
        assert!(!e.update_all(missing));
        assert_eq!(titles(&e), ["a", "b"], "nothing changed");
    }

    #[test]
    fn undoing_back_to_the_saved_show_is_not_dirty() {
        let mut e = Editor::new(Show::default());
        e.insert(0, sound("a"));
        assert!(e.is_dirty());
        e.undo();
        assert!(!e.is_dirty(), "back to the loaded show");
        e.redo();
        e.mark_saved();
        assert!(!e.is_dirty());
        e.renumber();
        assert!(e.is_dirty());
        e.undo();
        assert!(!e.is_dirty(), "back to the saved show");
        e.undo();
        assert!(e.is_dirty(), "before the save");
    }

    #[test]
    fn changing_a_cue_back_by_hand_is_not_dirty() {
        let mut e = Editor::new(Show::default());
        let a = e.insert(0, sound("a"));
        e.mark_saved();
        let original = e.show().cue(a).unwrap().clone();
        let mut renamed = original.clone();
        renamed.title = "b".into();
        e.update(renamed);
        assert!(e.is_dirty());
        e.update(original);
        assert!(!e.is_dirty(), "same as saved, though two edits are on the undo stack");
    }

    #[test]
    fn delete_removes_fade_references() {
        let mut e = Editor::new(Show::default());
        let a = e.insert(0, sound("a"));
        let b = e.insert(1, sound("b"));
        e.insert(
            2,
            Cue::new(CueKind::Volume(VolumeCue {
                targets: Targets::Cues(vec![a, b]),
                action: VolumeAction::FadeStop,
                volume_db: None,
                duration: None,
                curve: FadeCurve::Linear,
            })),
        );
        e.insert(
            3,
            Cue::new(CueKind::ExitLoop(ExitLoopCue {
                targets: Targets::Cues(vec![a]),
                exit: ExitLoop { mode: DevampMode::Immediate, ..Default::default() },
            })),
        );
        e.insert(4, Cue::new(CueKind::control(ControlKind::Pan, Targets::Cues(vec![b, a]))));
        e.delete(&[a]);
        let CueKind::Volume(f) = &e.show().cues[1].kind else { panic!() };
        assert_eq!(f.targets, Targets::Cues(vec![b]));
        let CueKind::ExitLoop(d) = &e.show().cues[2].kind else { panic!() };
        assert_eq!(d.targets, Targets::Cues(vec![]));
        assert_eq!(e.show().cues[3].kind.targets(), Some(&Targets::Cues(vec![b])));
        e.undo();
        assert_eq!(e.show().cues[4].kind.targets(), Some(&Targets::Cues(vec![b, a])), "undo brings it back");
    }

    #[test]
    fn delete_keeps_go_to_targets_and_clears_loop_exits() {
        let mut e = Editor::new(Show::default());
        let a = e.insert(0, sound("a"));
        let mut devamp = Cue::new(CueKind::ExitLoop(ExitLoopCue {
            targets: Targets::Cues(vec![a]),
            exit: ExitLoop::default(),
        }));
        devamp.advance = Advance::new(AdvanceAt::LoopExit { of: Some(a) });
        e.insert(1, devamp);
        e.insert(2, Cue::new(CueKind::GoTo(GoToCue { target: Some(a), start: false })));
        e.delete(&[a]);
        assert_eq!(e.show().cues[0].advance.at, AdvanceAt::LoopExit { of: None });
        let CueKind::GoTo(g) = &e.show().cues[1].kind else { panic!() };
        assert_eq!(g.target, Some(a), "a missing target, not none");
    }

    fn group(mode: GroupMode) -> Cue {
        Cue::new(CueKind::Group(GroupCue { mode, cues: Vec::new() }))
    }

    fn children(e: &Editor, id: CueId) -> Vec<&str> {
        let Some(CueKind::Group(g)) = e.show().cue(id).map(|c| &c.kind) else { panic!() };
        g.cues.iter().map(|c| c.title.as_str()).collect()
    }

    #[test]
    fn groups_hold_cues_that_are_edited_in_place() {
        let mut e = Editor::new(Show::default());
        let g = e.insert(0, group(GroupMode::Sequence));
        let a = e.insert_into(g, None, sound("a")).unwrap();
        let b = e.insert_into(g, None, sound("b")).unwrap();
        e.insert_into(g, Some(0), sound("c")).unwrap();
        assert_eq!(children(&e, g), ["c", "a", "b"]);
        assert!(e.insert_into(a, None, sound("x")).is_none(), "only groups take cues");

        assert_eq!(e.show().next_after(a), Some(b), "sequence: siblings chain");
        assert_eq!(e.show().next_after(b), None);
        e.move_to(&[b], 0);
        assert_eq!(children(&e, g), ["b", "c", "a"]);

        let mut renamed = e.show().cue(a).unwrap().clone();
        renamed.title = "A".into();
        assert!(e.update(renamed));
        assert_eq!(children(&e, g), ["b", "c", "A"]);
        e.delete(&[b]);
        assert_eq!(children(&e, g), ["c", "A"]);
        assert!(e.undo());
        assert_eq!(children(&e, g), ["b", "c", "A"]);
    }

    #[test]
    fn all_at_once_groups_do_not_chain() {
        let mut e = Editor::new(Show::default());
        let g = e.insert(0, group(GroupMode::All));
        let a = e.insert_into(g, None, sound("a")).unwrap();
        e.insert_into(g, None, sound("b")).unwrap();
        assert_eq!(e.show().next_after(a), None);
        assert_eq!(e.show().next_after(g), None, "last top-level cue");
    }

    fn with_pre_wait(mut c: Cue, secs: f64) -> Cue {
        c.pre_wait = secs;
        c
    }

    #[test]
    fn ungrouping_puts_the_cues_in_the_groups_place() {
        let mut e = Editor::new(Show::default());
        e.insert(0, sound("a"));
        let mut seq = group(GroupMode::Sequence);
        seq.pre_wait = 1.0;
        seq.advance = Advance::new(AdvanceAt::After { secs: 2.0 });
        seq.hotkey = Some(crate::Chord::new("KeyG", "g"));
        let g = e.insert(1, seq);
        let b = e.insert_into(g, None, with_pre_wait(sound("b"), 0.5)).unwrap();
        let c = e.insert_into(g, None, sound("c")).unwrap();
        e.insert(2, sound("d"));
        let volume = |targets| Cue::new(CueKind::control(ControlKind::Volume, targets));
        let fade = e.insert(3, volume(Targets::Cues(vec![g, b])));
        let after = e.insert(2, volume(Targets::Previous(PreviousKeyword::Previous)));
        let goto = e.insert(5, Cue::new(CueKind::GoTo(GoToCue { target: Some(g), start: true })));

        assert_eq!(e.ungroup(&[g]), [b, c]);
        assert_eq!(titles(&e), ["a", "b", "c", "", "d", "", ""]);
        let cue = |id| e.show().cue(id).unwrap();
        assert_eq!((cue(b).pre_wait, cue(c).pre_wait), (1.5, 0.0), "the group's pre-wait comes first");
        assert_eq!(cue(c).advance, Advance::new(AdvanceAt::After { secs: 2.0 }), "the last continues");
        assert_eq!(cue(b).hotkey.as_ref().map(|k| k.code.as_str()), Some("KeyG"));
        assert_eq!(cue(fade).kind.targets(), Some(&Targets::Cues(vec![b, c])), "no target twice");
        assert_eq!(cue(after).kind.targets(), Some(&Targets::Cues(vec![b, c])), "not only the last");
        let CueKind::GoTo(target) = &cue(goto).kind else { panic!() };
        assert_eq!(target.target, Some(b));

        assert!(e.ungroup(&[b]).is_empty(), "only groups");
        e.undo();
        assert_eq!(titles(&e), ["a", "", "", "d", "", ""]);
    }

    #[test]
    fn ungrouping_an_all_at_once_group_chains_its_cues_at_their_start() {
        let mut e = Editor::new(Show::default());
        let mut all = group(GroupMode::All);
        all.pre_wait = 1.0;
        let g = e.insert(0, all);
        let inner = e.insert_into(g, None, group(GroupMode::Sequence)).unwrap();
        let a = e.insert_into(inner, None, sound("a")).unwrap();
        let b = e.insert_into(g, None, with_pre_wait(sound("b"), 2.0)).unwrap();
        let c = e.insert_into(g, None, with_pre_wait(sound("c"), 0.5)).unwrap();
        e.insert(1, sound("d"));

        // The inner group comes out of the outer one, then dissolves too.
        assert_eq!(e.ungroup(&[g, inner]), [a, b, c]);
        assert_eq!(titles(&e), ["a", "b", "c", "d"]);
        let cue = |id| e.show().cue(id).unwrap();
        let start = Advance::new(AdvanceAt::Start);
        assert_eq!([a, b].map(|id| cue(id).advance), [start, start]);
        assert_eq!(cue(c).advance, Advance::default(), "the group's");
        // From GO: a at 1 s, b at 3 s, c as soon as possible after b.
        assert_eq!([a, b, c].map(|id| cue(id).pre_wait), [1.0, 2.0, 0.0]);
        assert!(e.undo());
        assert_eq!(children(&e, g), ["", "b", "c"], "one undo step");
    }

    #[test]
    fn grouping_puts_the_cues_in_sequence_where_the_first_was() {
        let mut e = Editor::new(Show::default());
        let a = e.insert(0, sound("a"));
        let b = e.insert(1, sound("b"));
        e.insert(2, sound("c"));
        let d = e.insert(3, sound("d"));
        e.renumber();
        let mut first = e.show().cue(b).unwrap().clone();
        first.hotkey = Some(crate::Chord::new("KeyB", "b"));
        let mut last = e.show().cue(d).unwrap().clone();
        last.hotkey = Some(crate::Chord::new("KeyD", "d"));
        last.advance = Advance::new(AdvanceAt::After { secs: 2.0 });
        e.update_all(vec![first, last]);
        let previous =
            || Cue::new(CueKind::control(ControlKind::Volume, Targets::Previous(PreviousKeyword::Previous)));
        let fade_b = e.insert(2, previous());
        let fade_d = e.insert(5, previous());

        let g = e.group(&[d, b, fade_b]).unwrap();
        assert_eq!(titles(&e), ["a", "Group", "c", ""]);
        assert_eq!(children(&e, g), ["b", "", "d"], "in list order");
        let cue = |id| e.show().cue(id).unwrap();
        let CueKind::Group(group) = &cue(g).kind else { panic!() };
        assert_eq!(group.mode, GroupMode::Sequence);
        assert_eq!(cue(g).number, "1.5", "between a and b");
        assert_eq!(cue(g).advance, Advance::new(AdvanceAt::After { secs: 2.0 }), "continues like the last");
        assert_eq!(cue(g).hotkey.as_ref().map(|k| k.code.as_str()), Some("KeyB"));
        assert_eq!((cue(b).hotkey.as_ref(), cue(d).hotkey.as_ref()), (None, None));
        assert_eq!(
            cue(fade_b).kind.targets(),
            Some(&Targets::Previous(PreviousKeyword::Previous)),
            "still b"
        );
        assert_eq!(cue(fade_d).kind.targets(), Some(&Targets::Cues(vec![d])), "not the whole group");

        // Groups don't nest.
        let revision = e.revision();
        assert_eq!(e.group(&[b]), Err(t!("edit.group_main_only")));
        assert_eq!(e.group(&[a, g]), Err(t!("edit.group_not_nested")));
        assert_eq!(e.revision(), revision, "nothing changed");

        // Dissolving brings back what grouping took.
        e.ungroup(&[g]);
        assert_eq!(titles(&e), ["a", "b", "", "d", "c", ""]);
        assert_eq!(e.show().cue(b).unwrap().hotkey.as_ref().map(|k| k.code.as_str()), Some("KeyB"));
        assert_eq!(e.show().cue(d).unwrap().advance, Advance::new(AdvanceAt::After { secs: 2.0 }));
    }

    #[test]
    fn scene_markers_are_not_grouped() {
        let mut e = Editor::new(Show::default());
        let act = e.insert(0, Cue::new(CueKind::Scene(crate::model::SceneCue {})));
        let a = e.insert(1, sound("a"));
        let revision = e.revision();
        assert!(e.group(&[act, a]).is_err());
        assert!(e.group(&[]).is_err());
        assert_eq!(e.revision(), revision, "nothing changed");
    }

    #[test]
    fn renumbering_leaves_scene_markers_without_a_number() {
        let mut e = Editor::new(Show::default());
        let mut act = Cue::new(CueKind::Scene(crate::model::SceneCue {}));
        act.number = "7".into();
        e.insert(0, act);
        let a = e.insert(1, sound("a"));
        e.insert(2, Cue::new(CueKind::Scene(crate::model::SceneCue {})));
        let b = e.insert(3, sound("b"));
        e.renumber();
        let numbers: Vec<_> = e.show().cues.iter().map(|c| c.number.as_str()).collect();
        assert_eq!(numbers, ["", "1", "", "2"]);
        assert_eq!(e.show().next_after(a), Some(b), "chains pass the marker");
        assert_eq!(e.show().playable_from(0), Some(1));
        assert_eq!(e.show().playable_from(2), Some(3));
        assert_eq!(e.show().playable_from(4), None);
    }

    #[test]
    fn renumbering_prefixes_groups_cues_with_the_group_number() {
        let mut e = Editor::new(Show::default());
        let a = e.insert(0, sound("a"));
        let g = e.insert(1, group(GroupMode::Sequence));
        let b = e.insert_into(g, None, sound("b")).unwrap();
        let inner = e.insert_into(g, None, group(GroupMode::All)).unwrap();
        let c = e.insert_into(inner, None, sound("c")).unwrap();
        let d = e.insert(2, sound("d"));
        e.renumber();
        let number = |id| e.show().cue(id).unwrap().number.clone();
        assert_eq!([a, g, b, inner, c, d].map(number), ["1", "2", "2.1", "2.2", "2.2.1", "3"]);
    }

    #[test]
    fn duplicate_gets_a_number_after_the_original() {
        let mut e = Editor::new(Show::default());
        let a = e.insert(0, sound("a"));
        e.insert(1, sound("b"));
        e.renumber();
        let copy = e.duplicate(a, false).unwrap();
        assert_eq!(e.show().cue(copy).map(|c| c.number.as_str()), Some("1.5"));
        let copy = e.duplicate(copy, false).unwrap();
        assert_eq!(e.show().cue(copy).map(|c| c.number.as_str()), Some("1.75"));
    }

    #[test]
    fn duplicating_a_group_renews_ids_and_inner_references() {
        let mut e = Editor::new(Show::default());
        let g = e.insert(0, group(GroupMode::Sequence));
        let a = e.insert_into(g, None, sound("a")).unwrap();
        let mut devamp = Cue::new(CueKind::ExitLoop(ExitLoopCue {
            targets: Targets::Cues(vec![a]),
            exit: ExitLoop { mode: DevampMode::LoopEnd, ..Default::default() },
        }));
        devamp.advance = Advance::new(AdvanceAt::LoopExit { of: Some(a) });
        e.insert_into(g, None, devamp).unwrap();
        let copy = e.duplicate(g, false).unwrap();
        let Some(CueKind::Group(c)) = e.show().cue(copy).map(|c| &c.kind) else { panic!() };
        let (new_a, devamp) = (c.cues[0].id, &c.cues[1]);
        assert_ne!(new_a, a);
        let CueKind::ExitLoop(d) = &devamp.kind else { panic!() };
        assert_eq!(d.targets, Targets::Cues(vec![new_a]), "points at the copy's sound, not the original");
        assert_eq!(devamp.advance.at, AdvanceAt::LoopExit { of: Some(new_a) });
        assert_eq!(e.show().cue(a).map(|c| c.title.as_str()), Some("a"), "original untouched");
        // Deleting the group removes what is inside.
        e.delete(&[copy]);
        assert!(e.show().cue(new_a).is_none());
    }

    fn sound_of(e: &Editor, id: CueId) -> &SoundCue {
        let Some(CueKind::Sound(s)) = e.show().cue(id).map(|c| &c.kind) else { panic!() };
        s
    }

    fn with_volume(e: &Editor, id: CueId, volume_db: f64) -> Cue {
        let mut c = e.show().cue(id).unwrap().clone();
        let CueKind::Sound(s) = &mut c.kind else { panic!() };
        s.volume_db = volume_db;
        c
    }

    #[test]
    fn linked_cues_share_all_but_their_instance_fields() {
        let mut e = Editor::new(Show::default());
        let a = e.insert(0, sound("a"));
        let b = e.insert(1, sound("b"));
        let c = e.insert(2, sound("c"));
        let mut template = with_volume(&e, a, -6.0);
        template.color = Some("#5b4a8a".into());
        e.update(template);
        e.link(a, &[b]).unwrap();
        assert_eq!(sound_of(&e, b).file, "a.wav", "takes the source's settings");
        assert_eq!(sound_of(&e, b).volume_db, -6.0);
        assert_eq!(e.show().cue(b).unwrap().color, None, "the color belongs to the instance");

        let mut edit = with_volume(&e, b, -3.0);
        edit.title = "B".into();
        edit.color = Some("#8a4a4a".into());
        edit.notes = "auf Ansage".into();
        edit.hotkey = Some(crate::Chord::new("KeyX", "x"));
        edit.pre_wait = 2.0;
        e.update(edit);
        let first = e.show().cue(a).unwrap();
        assert_eq!(sound_of(&e, a).volume_db, -3.0);
        assert_eq!(first.pre_wait, 2.0);
        assert_eq!((first.title.as_str(), first.notes.as_str(), first.hotkey.as_ref()), ("a", "", None));
        assert_eq!(first.color.as_deref(), Some("#5b4a8a"));
        assert_eq!(sound_of(&e, c).volume_db, 0.0, "not linked");

        e.undo();
        assert_eq!((sound_of(&e, a).volume_db, sound_of(&e, b).volume_db), (-6.0, -6.0), "one undo step");
    }

    #[test]
    fn edits_through_change_reach_linked_cues() {
        let mut e = Editor::new(Show::default());
        let a = e.insert(0, sound("a"));
        let b = e.insert(1, sound("b"));
        e.link(a, &[b]).unwrap();
        e.change(|s| {
            if let Some(Cue { kind: CueKind::Sound(sound), .. }) = s.cue_mut(b) {
                sound.file = "neu.wav".into();
            }
        });
        assert_eq!(sound_of(&e, a).file, "neu.wav");
    }

    #[test]
    fn update_keeps_the_link() {
        let mut e = Editor::new(Show::default());
        let a = e.insert(0, sound("a"));
        let b = e.insert(1, sound("b"));
        e.link(a, &[b]).unwrap();
        let mut stale = with_volume(&e, a, -10.0);
        stale.link = None;
        e.update(stale);
        assert!(e.show().cue(a).unwrap().link.is_some());
        assert_eq!(sound_of(&e, b).volume_db, -10.0);
    }

    #[test]
    fn links_need_two_cues_of_one_type_and_no_groups() {
        let mut e = Editor::new(Show::default());
        let a = e.insert(0, sound("a"));
        let w = e.insert(1, Cue::new(CueKind::Wait(crate::model::WaitCue { duration: 1.0 })));
        let g = e.insert(2, group(GroupMode::All));
        let g2 = e.insert(3, group(GroupMode::All));
        assert!(e.link(a, &[w]).is_err());
        assert!(e.link(g, &[g2]).is_err());
        assert!(e.link(a, &[a]).is_err());
        assert!(e.show().all().iter().all(|c| c.link.is_none()), "nothing linked");
    }

    #[test]
    fn linking_merges_links_and_unlinking_or_deleting_dissolves_single_ones() {
        let mut e = Editor::new(Show::default());
        let a = e.insert(0, sound("a"));
        let b = e.insert(1, sound("b"));
        let c = e.insert(2, sound("c"));
        let d = e.insert(3, sound("d"));
        e.link(a, &[b]).unwrap();
        e.link(c, &[d]).unwrap();
        e.link(c, &[a]).unwrap();
        let link = e.show().cue(c).unwrap().link;
        assert!([a, b, d].iter().all(|id| e.show().cue(*id).unwrap().link == link), "one link");
        assert_eq!(sound_of(&e, b).file, "c.wav", "b came along");

        e.unlink(&[a]);
        assert_eq!(e.show().cue(a).unwrap().link, None);
        assert_eq!(sound_of(&e, a).file, "c.wav", "keeps its settings");
        e.delete(&[b, c]);
        assert_eq!(e.show().cue(d).unwrap().link, None, "alone, so no longer linked");
    }

    #[test]
    fn linked_duplicates_and_copies_of_groups() {
        let mut e = Editor::new(Show::default());
        let a = e.insert(0, sound("a"));
        let plain = e.duplicate(a, false).unwrap();
        assert_eq!(e.show().cue(plain).unwrap().link, None);
        let copy = e.duplicate(a, true).unwrap();
        assert_eq!(e.show().linked(a), [copy]);
        e.update(with_volume(&e, copy, -1.0));
        assert_eq!(sound_of(&e, a).volume_db, -1.0);

        let g = e.insert(0, group(GroupMode::All));
        let inner = e.insert_into(g, None, sound("x")).unwrap();
        e.link(a, &[inner]).unwrap();
        assert_eq!(sound_of(&e, inner).file, "a.wav", "cues inside groups link too");
        let g_copy = e.duplicate(g, true).unwrap();
        assert_eq!(e.show().cue(g_copy).unwrap().link, None, "groups do not link");
        let Some(CueKind::Group(gr)) = e.show().cue(g_copy).map(|c| &c.kind) else { panic!() };
        assert_eq!(gr.cues[0].link, None, "copies inside are no instances");
    }

    #[test]
    fn sync_repairs_loaded_links() {
        let link = Some(LinkId::new());
        let mut show = Show::default();
        for (n, kind) in [
            ("a", CueKind::Sound(SoundCue::new("a.wav"))),
            ("b", CueKind::Sound(SoundCue::new("b.wav"))),
            ("w", CueKind::Wait(crate::model::WaitCue { duration: 1.0 })),
            ("g", CueKind::Group(GroupCue::default())),
        ] {
            let mut c = Cue::new(kind);
            c.title = n.into();
            c.link = link;
            show.cues.push(c);
        }
        let repaired = show.sync_links(None);
        assert_eq!(repaired, [show.cues[1].id]);
        let CueKind::Sound(b) = &show.cues[1].kind else { panic!() };
        assert_eq!(b.file, "a.wav", "the first one wins");
        assert_eq!(show.cues[1].link, link);
        assert_eq!((show.cues[2].link, show.cues[3].link), (None, None), "other type, group");
    }
}
