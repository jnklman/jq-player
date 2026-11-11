use std::collections::HashMap;

use jq_i18n::t;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const FORMAT: &str = "jqshow";
pub const VERSION: u32 = 1;

/// Stable internal cue identity; references between cues use it, never the number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CueId(pub Uuid);

impl CueId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for CueId {
    fn default() -> Self {
        Self::new()
    }
}

/// Cues sharing one are instances of each other: the editor keeps
/// everything but their identity, labels and list position identical (see
/// [`Cue::copy_shared`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LinkId(pub Uuid);

impl LinkId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for LinkId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Show {
    pub format: String,
    pub version: u32,
    /// Stable identity; this machine's settings for the show (output
    /// channels, OSC addresses) are filed under it.
    pub id: Uuid,
    /// Name shown in the app; independent of the show file and folder names.
    pub name: String,
    /// Media folder, relative to the show file's folder.
    #[serde(default = "default_media_root", skip_serializing_if = "omit_default_media_root")]
    pub media_root: String,
    #[serde(default = "default_outputs", skip_serializing_if = "omit_default_outputs")]
    pub outputs: Vec<Output>,
    /// What a cue with media does to the cues already running when it
    /// starts, unless it has a setting of its own ([`Cue::others`]).
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub others: OthersEffect,
    /// Keeps the show in show mode until unlocked (against slipping into
    /// edit mode during a performance).
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub show_mode_locked: bool,
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub level_limits: LevelLimits,
    /// Loudness that normalized cues are brought to, LUFS.
    #[serde(default = "default_loudness_target", skip_serializing_if = "omit_default_loudness_target")]
    pub loudness_target: f64,
    /// New sound and playlist cues are normalized as soon as their files are
    /// measured.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub normalize_on_import: bool,
    /// Network targets of OSC cues; a machine can override the address in
    /// its settings.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub devices: Vec<NetDevice>,
    /// Named stopwatches (performance time, interval length); their time
    /// runs in the show controller, for the session only.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub stopwatches: Vec<Stopwatch>,
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub cues: Vec<Cue>,
}

/// Volume that is silence whatever the limits (the bottom of a fader).
pub const SILENCE_DB: f64 = -120.0;

impl Cue {
    /// The volumes this cue and a group's cues set, by cue: a sound's or
    /// playlist's volume, a fade's target (without one it fades to silence).
    pub fn levels(&self) -> Vec<(CueId, f64)> {
        let mut cue = self.clone();
        cue.levels_mut().into_iter().map(|(id, db)| (id, *db)).collect()
    }

    fn levels_mut(&mut self) -> Vec<(CueId, &mut f64)> {
        let id = self.id;
        match &mut self.kind {
            CueKind::Sound(s) => vec![(id, &mut s.volume_db)],
            CueKind::Playlist(p) => vec![(id, &mut p.volume_db)],
            CueKind::Volume(f) => f.volume_db.as_mut().map(|db| (id, db)).into_iter().collect(),
            CueKind::Group(g) => g.cues.iter_mut().flat_map(Cue::levels_mut).collect(),
            _ => Vec::new(),
        }
    }

    /// Lowers volumes above `max_db` to it, those that differ from `before`
    /// (the cue as it was; `None` = all): a volume read from a show file
    /// stays as it is (it plays at the maximum) until it is changed.
    pub fn cap_changed_levels(&mut self, before: Option<&Cue>, max_db: f64) {
        let old: HashMap<CueId, f64> = before.map(|b| b.levels().into_iter().collect()).unwrap_or_default();
        for (id, db) in self.levels_mut() {
            if old.get(&id) != Some(db) {
                *db = db.min(max_db);
            }
        }
    }
}

/// What [`Show::clamp_levels`] changed (cues inside groups by their own id).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct LevelChanges {
    /// Lowered (or raised, if they played capped at a lower maximum before)
    /// to the new maximum.
    pub capped: Vec<CueId>,
    /// Audible before, silent now.
    pub silenced: Vec<CueId>,
}

impl Show {
    /// Brings every volume into new limits after the old ones: above the
    /// maximum to it; at or below either minimum to [`SILENCE_DB`], so what
    /// was silent stays so when the minimum goes down, and what the new one
    /// silences stays so when it goes down again later.
    pub fn clamp_levels(&mut self, old: LevelLimits, new: LevelLimits) -> LevelChanges {
        let mut changes = LevelChanges::default();
        for (id, db) in self.cues.iter_mut().flat_map(Cue::levels_mut) {
            if *db > new.max_db {
                *db = new.max_db;
                changes.capped.push(id);
            } else if *db > SILENCE_DB && (*db <= new.min_db || *db <= old.min_db) {
                if *db > old.min_db {
                    changes.silenced.push(id);
                }
                *db = SILENCE_DB;
            }
        }
        changes.capped.dedup();
        changes.silenced.dedup();
        changes
    }
}

/// Loudest and quietest level a cue plays at, dB.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LevelLimits {
    /// Nothing plays louder, whatever the cue says.
    pub max_db: f64,
    /// A volume at or below this is silence.
    pub min_db: f64,
}

impl Default for LevelLimits {
    fn default() -> Self {
        Self { max_db: 6.0, min_db: -60.0 }
    }
}

impl LevelLimits {
    /// True if a volume is below the minimum without being set to silence
    /// ([`SILENCE_DB`]): silent, though it was hardly meant to be.
    pub fn silences(&self, volume_db: f64) -> bool {
        volume_db > SILENCE_DB && volume_db <= self.min_db
    }

    /// True if a volume plus normalization offset plays capped at `max_db`.
    pub fn caps(&self, volume_db: f64, normalize_db: f64) -> bool {
        self.level(volume_db, normalize_db).is_some_and(|db| db < volume_db + normalize_db)
    }

    /// The level played for a cue volume plus its normalization offset:
    /// at most `max_db`; `None` (silence) for a volume at or below
    /// `min_db`, whatever the offset.
    pub fn level(&self, volume_db: f64, normalize_db: f64) -> Option<f64> {
        // NaN is silence too.
        if volume_db.is_nan() || volume_db <= self.min_db {
            return None;
        }
        let db = volume_db + if normalize_db.is_finite() { normalize_db } else { 0.0 };
        Some(db.min(self.max_db))
    }
}

fn default_loudness_target() -> f64 {
    -20.0
}

fn default_media_root() -> String {
    "media".into()
}

fn default_outputs() -> Vec<Output> {
    vec![Output { id: Output::MAIN.into(), name: "Main".into() }]
}

impl Default for Show {
    fn default() -> Self {
        Self {
            format: FORMAT.into(),
            version: VERSION,
            id: Uuid::new_v4(),
            name: String::new(),
            media_root: default_media_root(),
            outputs: default_outputs(),
            others: OthersEffect::default(),
            show_mode_locked: false,
            level_limits: LevelLimits::default(),
            loudness_target: default_loudness_target(),
            normalize_on_import: false,
            devices: Vec::new(),
            stopwatches: Vec::new(),
            cues: Vec::new(),
        }
    }
}

impl Show {
    /// The cue with `id`, also inside groups.
    pub fn cue(&self, id: CueId) -> Option<&Cue> {
        find(&self.cues, id)
    }

    pub fn cue_mut(&mut self, id: CueId) -> Option<&mut Cue> {
        find_mut(&mut self.cues, id)
    }

    /// Every cue, groups' cues included (each group before its cues).
    pub fn all(&self) -> Vec<&Cue> {
        fn walk<'a>(cues: &'a [Cue], out: &mut Vec<&'a Cue>) {
            for c in cues {
                out.push(c);
                if let CueKind::Group(g) = &c.kind {
                    walk(&g.cues, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.cues, &mut out);
        out
    }

    /// Index in the top-level list (the playhead's list).
    pub fn index_of(&self, id: CueId) -> Option<usize> {
        self.cues.iter().position(|c| c.id == id)
    }

    /// The list holding cue `id`: the show's or a group's.
    pub fn siblings(&self, id: CueId) -> Option<&Vec<Cue>> {
        siblings(&self.cues, id)
    }

    pub fn siblings_mut(&mut self, id: CueId) -> Option<&mut Vec<Cue>> {
        siblings_mut(&mut self.cues, id)
    }

    /// The cue that `advance` starts after `id`: its next sibling, past
    /// scene markers. Children of a group that starts all at once do not chain.
    pub fn next_after(&self, id: CueId) -> Option<CueId> {
        if let Some(group) = self.parent(id)
            && let CueKind::Group(g) = &group.kind
            && g.mode == GroupMode::All
        {
            return None;
        }
        let list = self.siblings(id)?;
        let i = list.iter().position(|c| c.id == id)?;
        list[i + 1..].iter().find(|c| !c.is_scene()).map(|c| c.id)
    }

    /// Index of the first top-level cue from `index` on that the playhead
    /// can stand on (scene markers are passed over).
    pub fn playable_from(&self, index: usize) -> Option<usize> {
        self.cues.iter().skip(index).position(|c| !c.is_scene()).map(|i| i + index)
    }

    /// The sibling directly before `id`.
    pub fn previous_before(&self, id: CueId) -> Option<CueId> {
        let list = self.siblings(id)?;
        let i = list.iter().position(|c| c.id == id)?;
        list.get(i.checked_sub(1)?).map(|c| c.id)
    }

    /// The nearest previous sibling that can receive control-cue changes.
    pub fn previous_control_target_before(&self, id: CueId) -> Option<CueId> {
        fn accepts_changes(cue: &Cue) -> bool {
            match &cue.kind {
                CueKind::Sound(_) | CueKind::Playlist(_) => true,
                CueKind::Group(group) => group.cues.iter().any(accepts_changes),
                _ => false,
            }
        }

        let list = self.siblings(id)?;
        let i = list.iter().position(|c| c.id == id)?;
        list[..i].iter().rev().find(|cue| accepts_changes(cue)).map(|cue| cue.id)
    }

    /// The group directly containing `id`.
    pub fn parent(&self, id: CueId) -> Option<&Cue> {
        fn walk(cues: &[Cue], id: CueId) -> Option<&Cue> {
            cues.iter().find_map(|c| match &c.kind {
                CueKind::Group(g) if g.cues.iter().any(|k| k.id == id) => Some(c),
                CueKind::Group(g) => walk(&g.cues, id),
                _ => None,
            })
        }
        walk(&self.cues, id)
    }

    /// The other cues linked with `id`, in list order.
    pub fn linked(&self, id: CueId) -> Vec<CueId> {
        let Some(link) = self.cue(id).and_then(|c| c.link) else { return Vec::new() };
        self.all().into_iter().filter(|c| c.link == Some(link) && c.id != id).map(|c| c.id).collect()
    }

    /// Copies the shared part of cue `id` to every cue linked with it.
    pub fn spread_link(&mut self, id: CueId) {
        let Some(source) = self.cue(id).cloned() else { return };
        for other in self.linked(id) {
            if let Some(c) = self.cue_mut(other) {
                c.copy_shared(&source);
            }
        }
    }

    /// Makes linked cues consistent after an edit from `before` (or after
    /// loading, `None`). Per link the source is the first cue, in list
    /// order, whose shared part changed since `before`, else the first one
    /// that existed before, else the first one; the others take its shared
    /// part. Groups, cues of another type than the source and links left
    /// with a single cue lose the link. Returns the cues that were changed.
    pub fn sync_links(&mut self, before: Option<&Show>) -> Vec<CueId> {
        let all = self.all();
        if all.iter().all(|c| c.link.is_none()) {
            return Vec::new();
        }
        let old: HashMap<CueId, &Cue> =
            before.map(|b| b.all().into_iter().map(|c| (c.id, c)).collect()).unwrap_or_default();
        let mut links: Vec<(LinkId, Vec<&Cue>)> = Vec::new();
        for c in all.into_iter().filter(|c| c.link.is_some()) {
            match links.iter_mut().find(|(l, _)| Some(*l) == c.link) {
                Some((_, members)) => members.push(c),
                None => links.push((c.link.unwrap_or_default(), vec![c])),
            }
        }
        // (cue, source to copy from; None = drop the link)
        let mut plan: Vec<(CueId, Option<Cue>)> = Vec::new();
        for (_, members) in links {
            let linkable: Vec<&Cue> = members.iter().copied().filter(|c| c.can_link()).collect();
            let changed = |c: &&Cue| old.get(&c.id).is_some_and(|o| !o.shares_with(c));
            let source = linkable
                .iter()
                .find(|c| changed(c))
                .or_else(|| linkable.iter().find(|c| old.contains_key(&c.id)))
                .or(linkable.first())
                .copied();
            let same_type: Vec<&Cue> = match source {
                Some(s) => {
                    linkable.iter().copied().filter(|c| c.kind.type_name() == s.kind.type_name()).collect()
                }
                None => Vec::new(),
            };
            for c in &members {
                if same_type.len() < 2 || !same_type.iter().any(|k| k.id == c.id) {
                    plan.push((c.id, None));
                } else if let Some(s) = source.filter(|s| !s.shares_with(c)) {
                    plan.push((c.id, Some(s.clone())));
                }
            }
        }
        let mut changed = Vec::new();
        for (id, source) in plan {
            let Some(c) = self.cue_mut(id) else { continue };
            match source {
                Some(s) => {
                    c.copy_shared(&s);
                    changed.push(id);
                }
                None => c.link = None,
            }
        }
        changed
    }
}

fn find(cues: &[Cue], id: CueId) -> Option<&Cue> {
    cues.iter().find_map(|c| match &c.kind {
        _ if c.id == id => Some(c),
        CueKind::Group(g) => find(&g.cues, id),
        _ => None,
    })
}

fn find_mut(cues: &mut [Cue], id: CueId) -> Option<&mut Cue> {
    for c in cues {
        if c.id == id {
            return Some(c);
        }
        if let CueKind::Group(g) = &mut c.kind
            && let Some(found) = find_mut(&mut g.cues, id)
        {
            return Some(found);
        }
    }
    None
}

fn siblings(cues: &Vec<Cue>, id: CueId) -> Option<&Vec<Cue>> {
    if cues.iter().any(|c| c.id == id) {
        return Some(cues);
    }
    cues.iter().find_map(|c| match &c.kind {
        CueKind::Group(g) => siblings(&g.cues, id),
        _ => None,
    })
}

fn siblings_mut(cues: &mut Vec<Cue>, id: CueId) -> Option<&mut Vec<Cue>> {
    if cues.iter().any(|c| c.id == id) {
        return Some(cues);
    }
    for c in cues.iter_mut() {
        if let CueKind::Group(g) = &mut c.kind
            && let Some(found) = siblings_mut(&mut g.cues, id)
        {
            return Some(found);
        }
    }
    None
}

/// A named logical output. Which device and channels it maps to is machine
/// specific and lives in the machine's settings, never in the show.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Output {
    pub id: String,
    pub name: String,
}

impl Output {
    pub const MAIN: &str = "main";
}

/// A key with its modifiers, by its physical position: `code` is the
/// browser's `KeyboardEvent.code` ("KeyB", "Digit1", "Numpad1", "F5"), so
/// Shift or another keyboard layout never change which key it is. Cmd on a
/// Mac counts as Ctrl.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Chord {
    pub code: String,
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub ctrl: bool,
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub shift: bool,
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub alt: bool,
    /// Text of the key when it was recorded; only shown where the current
    /// layout cannot be asked.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub label: String,
}

impl Chord {
    pub fn new(code: &str, label: &str) -> Self {
        Self { code: code.into(), ctrl: false, shift: false, alt: false, label: label.into() }
    }

    pub fn ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }

    pub fn shift(mut self) -> Self {
        self.shift = true;
        self
    }

    /// The same key with the same modifiers (the label does not count).
    pub fn same(&self, other: &Chord) -> bool {
        (&self.code, self.ctrl, self.shift, self.alt) == (&other.code, other.ctrl, other.shift, other.alt)
    }

    /// How it is shown in messages, e.g. "Strg+Umschalt+F5".
    pub fn text(&self) -> String {
        let mut text = String::new();
        let modifiers =
            [(self.ctrl, t!("key.ctrl")), (self.alt, t!("key.alt")), (self.shift, t!("key.shift"))];
        for (on, name) in modifiers {
            if on {
                text.push_str(&name);
                text.push('+');
            }
        }
        let code = self.code.as_str();
        let key = match code {
            "Space" => t!("key.space"),
            "Escape" => "Esc".to_string(),
            _ if !self.label.trim().is_empty() => self.label.to_uppercase(),
            _ => code.strip_prefix("Key").or(code.strip_prefix("Digit")).unwrap_or(code).to_string(),
        };
        text.push_str(&key);
        text
    }
}

impl PartialEq for Chord {
    fn eq(&self, other: &Self) -> bool {
        self.same(other) && self.label == other.label
    }
}

/// A control on a MIDI device; channels are 0-based.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MidiTrigger {
    Note { channel: u8, note: u8 },
    Cc { channel: u8, controller: u8 },
    Program { channel: u8, program: u8 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cue {
    pub id: CueId,
    /// Free text such as "12", "12.5" or "P3"; unique is recommended, not enforced.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub number: String,
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub title: String,
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub script_ref: String,
    /// Multi-line notes for the operator.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub notes: String,
    /// List background colour, e.g. "#5b4a8a".
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub color: Option<String>,
    /// Whether and when the next cue starts by itself.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub advance: Advance,
    /// GOs the playhead stays on this cue for (Repeat; top level only).
    /// Not `repeat`, which a playlist has already.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub passes: Repeat,
    /// Seconds between GO and the actual start.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub pre_wait: f64,
    /// Key that starts this cue regardless of the playhead (main list only).
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub hotkey: Option<Chord>,
    /// MIDI control that starts this cue like its hotkey (main list only).
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub midi_trigger: Option<MidiTrigger>,
    /// What a GO (or direct start) does while the cue is already running.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub second_trigger: SecondTrigger,
    /// Releasing the hotkey or MIDI control fires the second trigger, so
    /// with "stop" the cue plays while the key is held.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub release_trigger: bool,
    /// Effect on the other running cues when this one starts; `None` = the
    /// show's [`Show::others`].
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub others: Option<OthersEffect>,
    /// Cues with the same link share everything [`Cue::copy_shared`] copies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<LinkId>,
    #[serde(flatten)]
    pub kind: CueKind,
}

impl Cue {
    /// Stored paths of the media files this cue plays (a group: all of its
    /// cues').
    pub fn media_files(&self) -> Vec<&String> {
        match &self.kind {
            CueKind::Sound(s) => (!s.is_placeholder()).then_some(&s.file).into_iter().collect(),
            CueKind::Playlist(p) => p.items.iter().map(|i| &i.file).collect(),
            CueKind::Group(g) => g.cues.iter().flat_map(Cue::media_files).collect(),
            _ => Vec::new(),
        }
    }

    pub fn media_files_mut(&mut self) -> Vec<&mut String> {
        match &mut self.kind {
            CueKind::Sound(s) => (!s.is_placeholder()).then_some(&mut s.file).into_iter().collect(),
            CueKind::Playlist(p) => p.items.iter_mut().map(|i| &mut i.file).collect(),
            CueKind::Group(g) => g.cues.iter_mut().flat_map(Cue::media_files_mut).collect(),
            _ => Vec::new(),
        }
    }

    /// Ids of all cues inside this one (a group's, recursively).
    pub fn descendants(&self) -> Vec<CueId> {
        match &self.kind {
            CueKind::Group(g) => {
                g.cues.iter().flat_map(|c| std::iter::once(c.id).chain(c.descendants())).collect()
            }
            _ => Vec::new(),
        }
    }

    /// Gives this cue and everything inside it new ids (for duplicates);
    /// references between them follow. Links are dropped: a copy is not
    /// an instance unless made one.
    pub fn renew_ids(&mut self) {
        let mut map = std::collections::HashMap::new();
        fn assign(c: &mut Cue, map: &mut std::collections::HashMap<CueId, CueId>) {
            let new = CueId::new();
            map.insert(c.id, new);
            c.id = new;
            c.link = None;
            if let CueKind::Group(g) = &mut c.kind {
                g.cues.iter_mut().for_each(|k| assign(k, map));
            }
        }
        fn remap(c: &mut Cue, map: &std::collections::HashMap<CueId, CueId>) {
            if let AdvanceAt::LoopExit { of: Some(of) } = &mut c.advance.at {
                *of = *map.get(of).unwrap_or(of);
            }
            if let Some(Targets::Cues(t)) = c.kind.targets_mut() {
                t.iter_mut().for_each(|id| *id = *map.get(id).unwrap_or(id));
            }
            match &mut c.kind {
                CueKind::GoTo(g) => g.target = g.target.map(|t| *map.get(&t).unwrap_or(&t)),
                CueKind::Group(g) => g.cues.iter_mut().for_each(|k| remap(k, map)),
                _ => {}
            }
        }
        assign(self, &mut map);
        remap(self, &map);
    }

    /// False if the cue may run until stopped: an endless loop or vamps, a
    /// repeating playlist, a group with such a cue inside.
    pub fn ends_by_itself(&self) -> bool {
        match &self.kind {
            CueKind::Sound(s) => s.vamps.is_empty() && s.loop_mode != LoopMode::Forever,
            CueKind::Playlist(p) => !p.repeat,
            CueKind::Group(g) => g.cues.iter().all(Cue::ends_by_itself),
            _ => true,
        }
    }

    /// Groups cannot be linked: their cues have identities of their own;
    /// scene markers have nothing to share.
    pub fn can_link(&self) -> bool {
        !matches!(self.kind, CueKind::Group(_) | CueKind::Scene(_))
    }

    /// A scene marker: a heading in the list that playback passes over.
    pub fn is_scene(&self) -> bool {
        matches!(self.kind, CueKind::Scene(_))
    }

    /// Takes the part of `from` that linked cues share. Number, title,
    /// script reference, notes, color, hotkey, MIDI trigger, advance, passes,
    /// second trigger (with its release) and the effect on others belong to
    /// the instance (its place in the list); the rest is shared.
    pub fn copy_shared(&mut self, from: &Cue) {
        // Spelled out so a new field has to be sorted in here.
        let Cue {
            id: _,
            number: _,
            title: _,
            script_ref: _,
            notes: _,
            color: _,
            advance: _,
            passes: _,
            pre_wait,
            hotkey: _,
            midi_trigger: _,
            second_trigger: _,
            release_trigger: _,
            others: _,
            link: _,
            kind,
        } = from;
        self.pre_wait = *pre_wait;
        self.kind.clone_from(kind);
    }

    /// True if `other` has the same shared part.
    pub fn shares_with(&self, other: &Cue) -> bool {
        self.pre_wait == other.pre_wait && self.kind == other.kind
    }

    pub fn new(kind: CueKind) -> Self {
        Self {
            id: CueId::new(),
            number: String::new(),
            title: String::new(),
            script_ref: String::new(),
            notes: String::new(),
            color: None,
            advance: Advance::default(),
            passes: Repeat::Off,
            pre_wait: 0.0,
            hotkey: None,
            midi_trigger: None,
            second_trigger: SecondTrigger::Ignore,
            release_trigger: false,
            others: None,
            link: None,
            kind,
        }
    }
}

/// What triggering a cue that is already running does. Only a GO or a direct
/// start (hotkey, OSC, MIDI) counts; auto-continue and auto-follow on a
/// running cue are skipped with a warning.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum SecondTrigger {
    /// Nothing happens, without a warning.
    #[default]
    Ignore,
    /// Pauses the cue, or resumes it if it is paused.
    Pause,
    Stop,
    /// Fades out with the cue's own fade-out time (FADE ALL time if 0) and stops.
    FadeOut,
    /// Stops the cue and starts it again from the top.
    Restart,
    /// Leaves its loop, like a devamp cue on it.
    ExitLoop {
        exit: ExitLoop,
    },
    /// Playlists only: goes on to the next item, like the running cues'
    /// "next" button.
    NextItem,
}

/// Effect of a starting cue on the cues already running (outside its own
/// tree and the cues started by the same GO).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OthersEffect {
    pub effect: OthersAction,
    /// Lowering while ducking, dB (negative).
    pub duck_db: f64,
    /// Seconds of the fade out, or of the duck and its return.
    pub fade: f64,
    pub scope: OthersScope,
}

impl Default for OthersEffect {
    fn default() -> Self {
        Self { effect: OthersAction::None, duck_db: -10.0, fade: 2.0, scope: OthersScope::All }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OthersAction {
    /// Others keep playing (cues layer).
    #[default]
    None,
    /// Exclusive: others fade out and stop.
    FadeStop,
    /// Others are lowered by `duck_db` while this cue runs, then come back.
    Duck,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OthersScope {
    #[default]
    All,
    /// Only cues on the same output.
    Output,
}

/// Whether the next cue starts by itself, and when.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Advance {
    pub start_next: bool,
    /// Kept without `start_next`, but without effect.
    pub at: AdvanceAt,
}

impl Advance {
    pub fn new(at: AdvanceAt) -> Self {
        Self { start_next: true, at }
    }
}

/// The moment the next cue starts. Points that do not suit the cue's type
/// are not refused but read as the nearest that does (see the runner).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "at", rename_all = "snake_case")]
pub enum AdvanceAt {
    /// Together with this cue.
    Start,
    /// Seconds after this cue started (post-wait).
    After { secs: f64 },
    /// Once this cue's fade-in is done.
    AfterFadeIn,
    /// Once this cue has ended.
    #[default]
    End,
    /// When this cue's end fade begins.
    BeforeFadeOut,
    /// Seconds before this cue's end.
    BeforeEnd { secs: f64 },
    /// When the devamp cue's target actually leaves its loop: the first
    /// target to do so (`None`) or this sound cue inside the target.
    LoopExit {
        #[serde(default, skip_serializing_if = "omit_in_file")]
        of: Option<CueId>,
    },
}

/// Stored as the number of passes (0 and 1 = off) or the string `"forever"`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Repeat {
    #[default]
    Off,
    /// At least 2.
    Times(u32),
    Forever,
}

impl Serialize for Repeat {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Repeat::Off => s.serialize_u32(0),
            Repeat::Times(n) => s.serialize_u32(*n),
            Repeat::Forever => s.serialize_str("forever"),
        }
    }
}

impl<'de> Deserialize<'de> for Repeat {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Count(u32),
            Word(String),
        }
        match Raw::deserialize(d)? {
            Raw::Count(0 | 1) => Ok(Repeat::Off),
            Raw::Count(n) => Ok(Repeat::Times(n)),
            Raw::Word(w) if w == "forever" => Ok(Repeat::Forever),
            Raw::Word(w) => Err(serde::de::Error::custom(format!("unknown repeat \"{w}\""))),
        }
    }
}

/// Type-specific part of a cue; the `type` field selects the variant.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CueKind {
    Sound(SoundCue),
    Volume(VolumeCue),
    ExitLoop(ExitLoopCue),
    StartStop(StartStopCue),
    Pause(PauseCue),
    Pan(PanCue),
    Rate(RateCue),
    Position(PositionCue),
    Reset(ResetCue),
    Stopwatch(StopwatchCue),
    Wait(WaitCue),
    Osc(OscCue),
    Midi(MidiCue),
    Playlist(PlaylistCue),
    Group(GroupCue),
    #[serde(rename = "goto")]
    GoTo(GoToCue),
    Scene(SceneCue),
}

impl CueKind {
    pub fn type_name(&self) -> &'static str {
        match self {
            CueKind::Sound(_) => "sound",
            CueKind::Volume(_) => "volume",
            CueKind::ExitLoop(_) => "exit_loop",
            CueKind::StartStop(_) => "start_stop",
            CueKind::Pause(_) => "pause",
            CueKind::Pan(_) => "pan",
            CueKind::Rate(_) => "rate",
            CueKind::Position(_) => "position",
            CueKind::Reset(_) => "reset",
            CueKind::Stopwatch(_) => "stopwatch",
            CueKind::Wait(_) => "wait",
            CueKind::Osc(_) => "osc",
            CueKind::Midi(_) => "midi",
            CueKind::Playlist(_) => "playlist",
            CueKind::Group(_) => "group",
            CueKind::GoTo(_) => "goto",
            CueKind::Scene(_) => "scene",
        }
    }

    /// The cues a control cue acts on.
    pub fn targets(&self) -> Option<&Targets> {
        match self {
            CueKind::Volume(c) => Some(&c.targets),
            CueKind::ExitLoop(c) => Some(&c.targets),
            CueKind::StartStop(c) => Some(&c.targets),
            CueKind::Pause(c) => Some(&c.targets),
            CueKind::Pan(c) => Some(&c.targets),
            CueKind::Rate(c) => Some(&c.targets),
            CueKind::Position(c) => Some(&c.targets),
            CueKind::Reset(c) => Some(&c.targets),
            _ => None,
        }
    }

    pub fn targets_mut(&mut self) -> Option<&mut Targets> {
        match self {
            CueKind::Volume(c) => Some(&mut c.targets),
            CueKind::ExitLoop(c) => Some(&mut c.targets),
            CueKind::StartStop(c) => Some(&mut c.targets),
            CueKind::Pause(c) => Some(&mut c.targets),
            CueKind::Pan(c) => Some(&mut c.targets),
            CueKind::Rate(c) => Some(&mut c.targets),
            CueKind::Position(c) => Some(&mut c.targets),
            CueKind::Reset(c) => Some(&mut c.targets),
            _ => None,
        }
    }

    /// A control cue of `kind` with the defaults for a new one.
    pub fn control(kind: ControlKind, targets: Targets) -> Self {
        match kind {
            ControlKind::StartStop => {
                CueKind::StartStop(StartStopCue { targets, action: StartStopAction::Start, from: None })
            }
            ControlKind::Pause => CueKind::Pause(PauseCue { targets, action: PauseAction::Pause }),
            ControlKind::Volume => CueKind::Volume(VolumeCue {
                targets,
                action: VolumeAction::FadeStop,
                volume_db: None,
                duration: None,
                curve: FadeCurve::default(),
            }),
            ControlKind::Pan => {
                CueKind::Pan(PanCue { targets, pan: 0.0, duration: 0.0, curve: PanCurve::default() })
            }
            ControlKind::Rate => CueKind::Rate(RateCue {
                targets,
                rate: Some(default_rate()),
                transpose: None,
                duration: 0.0,
                curve: PanCurve::default(),
            }),
            ControlKind::Position => CueKind::Position(PositionCue {
                targets,
                to: PositionTo::Absolute { secs: 0.0 },
                crossfade_ms: default_crossfade_ms(),
            }),
            ControlKind::ExitLoop => CueKind::ExitLoop(ExitLoopCue { targets, exit: ExitLoop::default() }),
            ControlKind::Reset => CueKind::Reset(ResetCue { targets, duration: 0.0 }),
            ControlKind::Stopwatch => {
                CueKind::Stopwatch(StopwatchCue { watch: None, action: StopwatchAction::Start })
            }
        }
    }

    /// Which control type this is, if any.
    pub fn control_kind(&self) -> Option<ControlKind> {
        Some(match self {
            CueKind::StartStop(_) => ControlKind::StartStop,
            CueKind::Pause(_) => ControlKind::Pause,
            CueKind::Volume(_) => ControlKind::Volume,
            CueKind::Pan(_) => ControlKind::Pan,
            CueKind::Rate(_) => ControlKind::Rate,
            CueKind::Position(_) => ControlKind::Position,
            CueKind::ExitLoop(_) => ControlKind::ExitLoop,
            CueKind::Reset(_) => ControlKind::Reset,
            CueKind::Stopwatch(_) => ControlKind::Stopwatch,
            _ => return None,
        })
    }

    /// The same cue as control type `kind`: targets, duration and curve
    /// carry over where both types have them. "All" and an output become
    /// the previous entry for a start cue, which cannot start those.
    pub fn convert(&self, kind: ControlKind) -> Option<CueKind> {
        self.control_kind()?;
        let mut targets = self.targets().cloned().unwrap_or(Targets::Previous(PreviousKeyword::Previous));
        if kind == ControlKind::StartStop && matches!(targets, Targets::All(_) | Targets::Output { .. }) {
            targets = Targets::Previous(PreviousKeyword::Previous);
        }
        let duration = match self {
            CueKind::Volume(v) => v.duration,
            CueKind::Pan(p) => Some(p.duration),
            CueKind::Rate(r) => Some(r.duration),
            CueKind::Reset(r) => Some(r.duration),
            _ => None,
        };
        let curve = match self {
            CueKind::Volume(v) => Some(v.curve),
            CueKind::Pan(PanCue { curve, .. }) | CueKind::Rate(RateCue { curve, .. }) => Some(match curve {
                PanCurve::Linear => FadeCurve::Linear,
                PanCurve::SCurve => FadeCurve::SCurve,
            }),
            _ => None,
        };
        let mut out = CueKind::control(kind, targets);
        match &mut out {
            CueKind::Volume(v) => {
                v.duration = duration;
                v.curve = curve.unwrap_or(v.curve);
            }
            CueKind::Pan(PanCue { duration: d, curve: c, .. })
            | CueKind::Rate(RateCue { duration: d, curve: c, .. }) => {
                *d = duration.unwrap_or(0.0);
                *c = match curve {
                    Some(FadeCurve::Linear | FadeCurve::Decibel) => PanCurve::Linear,
                    _ => PanCurve::SCurve,
                };
            }
            CueKind::Reset(r) => r.duration = duration.unwrap_or(0.0),
            CueKind::ExitLoop(e) => {
                if let CueKind::ExitLoop(old) = self {
                    e.exit = old.exit;
                }
            }
            _ => {}
        }
        Some(out)
    }
}

/// The control cue types, for adding one and switching between them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlKind {
    StartStop,
    Pause,
    Volume,
    Pan,
    Rate,
    Position,
    ExitLoop,
    Reset,
    Stopwatch,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FadeCurve {
    Linear,
    SCurve,
    /// Even steps in dB (to and from silence via the show's minimum level).
    Decibel,
    /// Smoothstep in dB: like `Decibel`, with a gentle start and end.
    DecibelS,
    /// In dB, slow near the louder level (a fade-out holds, then falls; a
    /// fade-in settles gently); the default.
    #[default]
    DecibelSlowTop,
    /// In dB, slow near the quieter level.
    DecibelSlowBottom,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SoundCue {
    /// Path relative to the media folder, always with `/` separators; empty
    /// for a placeholder ([`SoundCue::is_placeholder`]).
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub file: String,
    #[serde(default = "default_output", skip_serializing_if = "omit_default_output")]
    pub output: String,
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub volume_db: f64,
    /// Added to `volume_db` so the file plays at the show's loudness
    /// target; `None` = not normalized.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalize_db: Option<f64>,
    /// -1 (left) … +1 (right).
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub pan: f64,
    /// Start position in the file, seconds.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub start: f64,
    /// End position in the file, seconds; `None` = end of file.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub end: Option<f64>,
    /// Repeats of the start–end range; ignored while the cue has vamps.
    #[serde(default, rename = "loop", skip_serializing_if = "omit_in_file")]
    pub loop_mode: LoopMode,
    /// Regions repeated until a devamp cue releases them, in file order.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub vamps: Vec<Vamp>,
    /// Points in the file (seconds) where a devamp "at next mark" may leave
    /// a vamp, e.g. bar starts.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub marks: Vec<f64>,
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub fade_in: f64,
    /// Fade time used when this cue is faded without an explicit time, and
    /// for the end fade.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub fade_out: f64,
    /// Fade out automatically before the end position.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub end_fade: bool,
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub curve: FadeCurve,
    /// Rate, pitch preservation and transposition.
    #[serde(flatten)]
    pub speed: Speed,
}

impl SoundCue {
    /// A placeholder: a sound cue without a file yet (`file` empty), set in
    /// the list before the sound exists. It plays nothing.
    pub fn placeholder() -> Self {
        Self::new("")
    }

    pub fn is_placeholder(&self) -> bool {
        self.file.is_empty()
    }

    pub fn new(file: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            output: default_output(),
            volume_db: 0.0,
            normalize_db: None,
            pan: 0.0,
            start: 0.0,
            end: None,
            loop_mode: LoopMode::Off,
            vamps: Vec::new(),
            marks: Vec::new(),
            fade_in: 0.0,
            fade_out: 0.0,
            end_fade: false,
            curve: FadeCurve::default(),
            speed: Speed::default(),
        }
    }
}

/// A vamp region in the file, seconds.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vamp {
    pub start: f64,
    pub end: f64,
}

fn default_output() -> String {
    Output::MAIN.into()
}

/// Stored as the number of passes (0 and 1 = off) or the string `"forever"`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LoopMode {
    #[default]
    Off,
    /// Passes through the range, at least 2.
    Times(u32),
    Forever,
}

impl LoopMode {
    /// Jumps back to the start of the range; `None` = until stopped.
    pub fn repeats(self) -> Option<u32> {
        match self {
            LoopMode::Off => Some(0),
            LoopMode::Times(n) => Some(n.saturating_sub(1)),
            LoopMode::Forever => None,
        }
    }
}

impl Serialize for LoopMode {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            LoopMode::Off => s.serialize_u32(0),
            LoopMode::Times(n) => s.serialize_u32(*n),
            LoopMode::Forever => s.serialize_str("forever"),
        }
    }
}

impl<'de> Deserialize<'de> for LoopMode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Count(u32),
            Word(String),
        }
        match Raw::deserialize(d)? {
            Raw::Count(0 | 1) => Ok(LoopMode::Off),
            Raw::Count(n) => Ok(LoopMode::Times(n)),
            Raw::Word(w) if w == "forever" => Ok(LoopMode::Forever),
            Raw::Word(w) => Err(serde::de::Error::custom(format!("unknown loop mode \"{w}\""))),
        }
    }
}

/// Fades its targets to a level, or out and stops them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VolumeCue {
    pub targets: Targets,
    pub action: VolumeAction,
    /// Target level for [`VolumeAction::FadeTo`].
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub volume_db: Option<f64>,
    /// Overrides the target cue's own fade time; `None` uses it.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub duration: Option<f64>,
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub curve: FadeCurve,
}

/// What a control cue acts on: `"previous"`, `"all"`, `["<cue id>", …]` or
/// `{"output": "<output id>"}`. A group as target means everything in it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Targets {
    /// The entry directly before the control cue in its list, so linked
    /// control cues each act on their own predecessor.
    Previous(PreviousKeyword),
    All(AllKeyword),
    Cues(Vec<CueId>),
    Output {
        output: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviousKeyword {
    Previous,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllKeyword {
    All,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VolumeAction {
    FadeTo,
    FadeStop,
}

/// Leaves the current loop or vamp of running cues; with several vamps,
/// each one releases the next. No target running counts as success.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExitLoopCue {
    pub targets: Targets,
    #[serde(flatten)]
    pub exit: ExitLoop,
}

/// Starts or stops its targets without moving the playhead.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StartStopCue {
    pub targets: Targets,
    pub action: StartStopAction,
    /// Start sound cues from here (seconds in the file) instead of their
    /// start position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartStopAction {
    /// Starts targets that do not run, resumes paused ones.
    Start,
    Stop,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PauseCue {
    pub targets: Targets,
    pub action: PauseAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PauseAction {
    Pause,
    Resume,
    /// Pauses all targets if anything of them still plays, else resumes them.
    Toggle,
}

/// Moves the balance of its targets over a time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PanCue {
    pub targets: Targets,
    /// -1 (left) … +1 (right).
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub pan: f64,
    /// Seconds; 0 = at once (with a short smoothing).
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub duration: f64,
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub curve: PanCurve,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanCurve {
    Linear,
    #[default]
    SCurve,
}

/// Changes rate and transposition of its targets over a time; whether the
/// pitch follows the rate stays the target's own setting.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RateCue {
    pub targets: Targets,
    /// Percent (100 = original); `None` leaves the rate as it is.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub rate: Option<f64>,
    /// Semitones; `None` leaves the transposition as it is.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub transpose: Option<f64>,
    /// Seconds; 0 = at once (with a short smoothing).
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub duration: f64,
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub curve: PanCurve,
}

/// Jumps within running sound cues (a playlist: within its current item,
/// relative jumps only).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PositionCue {
    pub targets: Targets,
    pub to: PositionTo,
    /// Crossfade from the old position to the new one, ms.
    #[serde(default = "default_crossfade_ms", skip_serializing_if = "omit_default_crossfade_ms")]
    pub crossfade_ms: f64,
}

fn default_crossfade_ms() -> f64 {
    20.0
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "to", rename_all = "snake_case")]
pub enum PositionTo {
    /// Seconds in the file, like marks.
    Absolute {
        secs: f64,
    },
    /// Seconds ahead (negative: back) of the current position.
    Relative {
        secs: f64,
    },
    /// Mark number `index` of the cue, from 1.
    Mark {
        index: usize,
    },
    NextMark,
    PreviousMark,
}

/// Brings the level and pan of its targets back to their cues' own.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResetCue {
    pub targets: Targets,
    /// Seconds.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub duration: f64,
}

/// A named stopwatch of the show; with `countdown` it counts down.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stopwatch {
    pub id: String,
    pub name: String,
    /// Seconds a countdown starts from (> 0); it runs on past 0 into
    /// overtime. `None` = counts up from 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub countdown: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StopwatchCue {
    /// [`Stopwatch::id`]; `None` = not set yet.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub watch: Option<String>,
    pub action: StopwatchAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopwatchAction {
    /// Runs on from the time it shows.
    Start,
    /// Stands still; the time stays.
    Stop,
    /// Back to 0, standing still.
    Reset,
    /// Back to 0 and running.
    Restart,
}

/// How a loop is left: by a devamp cue or a second trigger.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExitLoop {
    pub mode: DevampMode,
    pub level: LoopLevel,
    /// The cue ends at the exit instead of playing on past the loop.
    pub stop: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopLevel {
    /// The first loop or vamp not left yet; a further trigger leaves the next.
    #[default]
    Innermost,
    /// Every loop and vamp at once.
    All,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevampMode {
    /// Finish the current pass, then play on (musically clean).
    #[default]
    LoopEnd,
    /// Leave at the next mark inside the vamp (e.g. a bar start).
    NextMark,
    /// Short crossfade to the exit right away (emergency, missed cue).
    Immediate,
}

/// Moves the playhead to another top-level cue (a group as a whole), and
/// starts it if so set: the only jump in the list.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GoToCue {
    /// Kept when the target is deleted (it then counts as missing).
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub target: Option<CueId>,
    /// Start the target like a GO on it, not only select it.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub start: bool,
}

/// A scene marker: a heading in the cue list (act, scene) that structures
/// it. Only its title, notes and colour count; playback passes over it (the
/// playhead never stands on it, a chain runs on to the cue after it).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneCue {}

/// A timed pause in a chain: the next cue starts when it has elapsed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WaitCue {
    /// Seconds.
    pub duration: f64,
}

/// A named network target, e.g. "Lichtpult" → 192.168.1.20:8000.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetDevice {
    pub id: String,
    pub name: String,
    /// IP address (or host name; resolving it may take a moment).
    pub host: String,
    pub port: u16,
}

/// Sends OSC messages over UDP to one device.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OscCue {
    /// [`NetDevice::id`]; `None` = not set yet.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub device: Option<String>,
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub messages: Vec<OscMessage>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OscMessage {
    /// e.g. `/pb/1/go`.
    pub address: String,
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub args: Vec<OscArg>,
}

/// Stored tagged, `{"int": 1}`, `{"float": 1.0}`, `{"string": "go"}`, so a
/// float 1.0 stays a float even through JavaScript.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OscArg {
    Int(i32),
    Float(f32),
    String(String),
}

/// Sends MIDI messages to the machine's MIDI output (machine settings).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MidiCue {
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub messages: Vec<MidiMessage>,
}

/// One outgoing message; channels are 0-based (shown as 1–16).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MidiMessage {
    ProgramChange {
        channel: u8,
        program: u8,
    },
    ControlChange {
        channel: u8,
        controller: u8,
        value: u8,
    },
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        note: u8,
    },
    /// MIDI Show Control, e.g. GO on cue 12.5 of a lighting desk.
    Msc {
        /// Device id of the receiver; 127 = all devices.
        device: u8,
        command: MscCommand,
        /// Cue number, digits and `.`; empty = the receiver's next cue.
        #[serde(default, skip_serializing_if = "omit_in_file")]
        cue: String,
        /// Optional cue list.
        #[serde(default, skip_serializing_if = "omit_in_file")]
        list: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MscCommand {
    Go,
    Stop,
    Resume,
}

/// Lowest and highest rate a cue plays at, percent.
pub const RATE_MIN: f64 = 50.0;
pub const RATE_MAX: f64 = 150.0;
/// Largest transposition up or down, semitones.
pub const TRANSPOSE_MAX: f64 = 12.0;

/// How fast and at what pitch a sound or playlist cue plays.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Speed {
    /// Percent of the original speed (100 = original), 50–150.
    #[serde(default = "default_rate", skip_serializing_if = "is_default_rate")]
    pub rate: f64,
    /// The pitch stays when the rate changes (like QLab); off, it follows
    /// the rate like a tape (varispeed).
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub preserve_pitch: bool,
    /// Semitones up (positive) or down, −12…+12, also fractions.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub transpose: f64,
}

impl Default for Speed {
    fn default() -> Self {
        Self { rate: default_rate(), preserve_pitch: true, transpose: 0.0 }
    }
}

impl Speed {
    /// Within the ranges playback allows (NaN as the default).
    pub fn clamped(self) -> Self {
        let rate = if self.rate.is_nan() { default_rate() } else { self.rate.clamp(RATE_MIN, RATE_MAX) };
        let transpose =
            if self.transpose.is_nan() { 0.0 } else { self.transpose.clamp(-TRANSPOSE_MAX, TRANSPOSE_MAX) };
        Self { rate, transpose, ..self }
    }

    /// Rate (file time per real time) and pitch (frequency factor) for the
    /// engine; equal for varispeed.
    pub fn factors(self) -> (f64, f64) {
        let s = self.clamped();
        let rate = s.rate / 100.0;
        let shift = 2f64.powf(s.transpose / 12.0);
        (rate, if s.preserve_pitch { shift } else { rate * shift })
    }

    /// True if the values are outside the ranges (a hand-edited file).
    pub fn out_of_range(self) -> bool {
        self.clamped() != self
    }
}

fn default_rate() -> f64 {
    100.0
}

fn is_default_rate(rate: &f64) -> bool {
    *rate == 100.0
}

fn default_true() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

fn is_zero(x: &f64) -> bool {
    *x == 0.0
}

thread_local! {
    /// Set while [`show_file_json`] runs on this thread.
    static WRITING_FILE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The show as written to its file: fields at the value they get when
/// missing are left out. Everywhere else (the UI) every field is sent.
pub(crate) fn show_file_json(show: &Show) -> Vec<u8> {
    WRITING_FILE.set(true);
    let json = serde_json::to_vec_pretty(show);
    WRITING_FILE.set(false);
    json.expect("show serializes")
}

/// For fields with `#[serde(default)]`; each must match what reading a
/// missing field gives, or a saved show would load changed.
fn omit_in_file<T: Default + PartialEq>(value: &T) -> bool {
    WRITING_FILE.get() && *value == T::default()
}

fn omit_default_media_root(value: &String) -> bool {
    WRITING_FILE.get() && *value == default_media_root()
}

fn omit_default_outputs(value: &Vec<Output>) -> bool {
    WRITING_FILE.get() && *value == default_outputs()
}

fn omit_default_loudness_target(value: &f64) -> bool {
    WRITING_FILE.get() && *value == default_loudness_target()
}

fn omit_default_output(value: &String) -> bool {
    WRITING_FILE.get() && *value == default_output()
}

fn omit_default_crossfade_ms(value: &f64) -> bool {
    WRITING_FILE.get() && *value == default_crossfade_ms()
}

/// Several files played one after the other as one cue (walk-in, interval).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlaylistCue {
    /// Played in this order.
    pub items: Vec<PlaylistItem>,
    #[serde(default = "default_output", skip_serializing_if = "omit_default_output")]
    pub output: String,
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub volume_db: f64,
    /// -1 (left) … +1 (right), for every item.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub pan: f64,
    /// Fade-in of every item.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub fade_in: f64,
    /// Fade time when the cue is faded without an explicit time.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub fade_out: f64,
    /// Silence between items, seconds.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub gap: f64,
    /// Start over after the last item.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub repeat: bool,
    /// Play the items in a random order instead of the list's.
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub shuffle: Shuffle,
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub curve: FadeCurve,
    /// Rate, pitch preservation and transposition of every item.
    #[serde(flatten)]
    pub speed: Speed,
}

/// One entry of a playlist: its file and what it plays with instead of the
/// playlist's settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlaylistItem {
    /// Path relative to the media folder.
    pub file: String,
    /// Normalization offset of the file, added to the volume; `None` = not
    /// normalized.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalize_db: Option<f64>,
    #[serde(flatten)]
    pub overrides: ItemOverrides,
}

impl PlaylistItem {
    pub fn new(file: impl Into<String>) -> Self {
        Self { file: file.into(), normalize_db: None, overrides: ItemOverrides::default() }
    }
}

/// What one playlist item plays with instead of the playlist's settings;
/// every field left out keeps the playlist's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ItemOverrides {
    /// Added to the playlist's volume (like a normalization offset), so the
    /// item stays this much louder or quieter when the playlist's volume is
    /// changed or faded.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub gain_db: f64,
    /// Start position in the file, seconds (a playlist plays its items
    /// from their start).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub start: f64,
    /// End position in the file, seconds; `None` = end of file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<f64>,
    /// -1 (left) … +1 (right).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pan: Option<f64>,
    /// Percent, like [`Speed::rate`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preserve_pitch: Option<bool>,
    /// Semitones, like [`Speed::transpose`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transpose: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fade_in: Option<f64>,
    /// Fade time when the item is faded without an explicit time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fade_out: Option<f64>,
}

impl ItemOverrides {
    /// `speed` with the overridden values instead.
    pub fn speed_over(&self, speed: Speed) -> Speed {
        Speed {
            rate: self.rate.unwrap_or(speed.rate),
            preserve_pitch: self.preserve_pitch.unwrap_or(speed.preserve_pitch),
            transpose: self.transpose.unwrap_or(speed.transpose),
        }
    }
}

impl PlaylistCue {
    pub fn new(files: Vec<String>) -> Self {
        Self {
            items: files.into_iter().map(PlaylistItem::new).collect(),
            output: default_output(),
            volume_db: 0.0,
            pan: 0.0,
            fade_in: 0.0,
            fade_out: 0.0,
            gap: 0.0,
            repeat: false,
            shuffle: Shuffle::Off,
            curve: FadeCurve::default(),
            speed: Speed::default(),
        }
    }

    /// Item `index` as a sound cue with the playlist's settings and the
    /// item's own; the volume includes the item's gain.
    pub fn item(&self, index: usize) -> Option<SoundCue> {
        let item = self.items.get(index)?;
        let o = &item.overrides;
        Some(SoundCue {
            output: self.output.clone(),
            volume_db: self.volume_db + o.gain_db,
            normalize_db: item.normalize_db,
            pan: o.pan.unwrap_or(self.pan),
            fade_in: o.fade_in.unwrap_or(self.fade_in),
            fade_out: o.fade_out.unwrap_or(self.fade_out),
            curve: self.curve,
            speed: o.speed_over(self.speed),
            start: o.start,
            end: o.end,
            ..SoundCue::new(item.file.clone())
        })
    }
}

/// Random playing order of a playlist.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shuffle {
    /// In the list's order.
    #[default]
    Off,
    /// Drawn anew at every start; a repeating playlist keeps it for all passes.
    PerStart,
    /// Drawn anew for every pass of a repeating playlist (a pass never
    /// starts with the item just played); without repeat like `PerStart`.
    PerPass,
}

impl Shuffle {
    pub fn is_off(&self) -> bool {
        *self == Shuffle::Off
    }
}

/// Cues that run as one: edited in a dialog, started by one GO.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GroupCue {
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub mode: GroupMode,
    #[serde(default, skip_serializing_if = "omit_in_file")]
    pub cues: Vec<Cue>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupMode {
    /// All cues start together; their advance settings do not apply.
    #[default]
    All,
    /// The first cue starts; the others follow by their advance settings.
    Sequence,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_playlist_item_plays_with_its_overrides() {
        let mut p = PlaylistCue { volume_db: -3.0, pan: -1.0, fade_in: 2.0, ..PlaylistCue::new(vec![]) };
        p.speed.rate = 90.0;
        let overrides = ItemOverrides {
            gain_db: -6.0,
            start: 1.5,
            end: Some(20.0),
            pan: Some(0.5),
            rate: Some(110.0),
            ..ItemOverrides::default()
        };
        p.items = vec![
            PlaylistItem::new("a.wav"),
            PlaylistItem { normalize_db: Some(2.0), overrides, ..PlaylistItem::new("b.wav") },
        ];
        let a = p.item(0).unwrap();
        assert_eq!(
            (a.volume_db, a.normalize_db, a.pan, a.fade_in, a.speed.rate),
            (-3.0, None, -1.0, 2.0, 90.0)
        );
        let b = p.item(1).unwrap();
        assert_eq!(
            (b.volume_db, b.normalize_db, b.pan, b.fade_in, b.speed.rate),
            (-9.0, Some(2.0), 0.5, 2.0, 110.0)
        );
        assert_eq!((a.start, a.end, b.start, b.end), (0.0, None, 1.5, Some(20.0)));
        assert!(b.speed.preserve_pitch, "what is not overridden stays the playlist's");
        // Only what is set is stored, next to the file.
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(
            json["items"],
            serde_json::json!([
                {"file": "a.wav"},
                {
                    "file": "b.wav", "normalize_db": 2.0, "gain_db": -6.0, "start": 1.5, "end": 20.0,
                    "pan": 0.5, "rate": 110.0
                }
            ])
        );
        assert_eq!(serde_json::from_value::<PlaylistCue>(json).unwrap(), p);
    }

    #[test]
    fn converting_a_control_cue_keeps_targets_duration_and_curve() {
        let id = CueId::new();
        let volume = CueKind::Volume(VolumeCue {
            targets: Targets::Cues(vec![id]),
            action: VolumeAction::FadeTo,
            volume_db: Some(-6.0),
            duration: Some(4.0),
            curve: FadeCurve::Linear,
        });
        let Some(CueKind::Pan(p)) = volume.convert(ControlKind::Pan) else { panic!() };
        assert_eq!((p.targets, p.duration, p.curve), (Targets::Cues(vec![id]), 4.0, PanCurve::Linear));
        let Some(CueKind::Reset(r)) = CueKind::Pan(PanCue {
            targets: Targets::All(AllKeyword::All),
            pan: 1.0,
            duration: 2.0,
            curve: PanCurve::SCurve,
        })
        .convert(ControlKind::Reset) else {
            panic!()
        };
        assert_eq!((r.targets, r.duration), (Targets::All(AllKeyword::All), 2.0));
        let all = CueKind::control(ControlKind::Pause, Targets::Output { output: "main".into() });
        let Some(CueKind::StartStop(s)) = all.convert(ControlKind::StartStop) else { panic!() };
        assert_eq!(s.targets, Targets::Previous(PreviousKeyword::Previous), "a start needs cues");
        assert_eq!(CueKind::Wait(WaitCue { duration: 1.0 }).convert(ControlKind::Pan), None);
    }

    #[test]
    fn previous_control_target_skips_cues_that_cannot_receive_changes() {
        let mut group = Cue::new(CueKind::Group(GroupCue::default()));
        let sound = Cue::new(CueKind::Sound(SoundCue::new("sound.wav")));
        if let CueKind::Group(g) = &mut group.kind {
            g.cues.push(Cue::new(CueKind::Group(GroupCue { mode: GroupMode::All, cues: vec![sound] })));
        }
        let empty_group = Cue::new(CueKind::Group(GroupCue::default()));
        let pause = Cue::new(CueKind::Pause(PauseCue {
            targets: Targets::Previous(PreviousKeyword::Previous),
            action: PauseAction::Pause,
        }));
        let target = Cue::new(CueKind::Pause(PauseCue {
            targets: Targets::Previous(PreviousKeyword::Previous),
            action: PauseAction::Pause,
        }));
        let show = Show { cues: vec![group.clone(), empty_group, pause, target.clone()], ..Show::default() };
        assert_eq!(show.previous_control_target_before(target.id), Some(group.id));

        let playlist = Cue::new(CueKind::Playlist(PlaylistCue::new(vec![])));
        let show = Show { cues: vec![group, playlist.clone(), target.clone()], ..Show::default() };
        assert_eq!(show.previous_control_target_before(target.id), Some(playlist.id));
    }

    #[test]
    fn new_pan_cues_use_the_s_curve() {
        let Some(CueKind::Pan(p)) = Some(CueKind::control(ControlKind::Pan, Targets::All(AllKeyword::All)))
        else {
            panic!()
        };
        assert_eq!(p.curve, PanCurve::SCurve);
    }

    #[test]
    fn speed_defaults_stay_out_of_the_file() {
        let sound = SoundCue::new("a.wav");
        let json = serde_json::to_value(&sound).unwrap();
        for key in ["rate", "preserve_pitch", "transpose", "speed"] {
            assert!(json.get(key).is_none(), "{key} in {json}");
        }
        let read: SoundCue = serde_json::from_value(serde_json::json!({ "file": "a.wav" })).unwrap();
        assert_eq!(read.speed, Speed::default());
        assert!(read.speed.preserve_pitch);
        let set = SoundCue { speed: Speed { rate: 80.0, preserve_pitch: false, transpose: -3.0 }, ..sound };
        let json = serde_json::to_value(&set).unwrap();
        assert_eq!(json["rate"], 80.0);
        assert_eq!(json["preserve_pitch"], false);
        assert_eq!(json["transpose"], -3.0);
        assert_eq!(serde_json::from_value::<SoundCue>(json).unwrap(), set);
    }

    #[test]
    fn speed_factors_for_the_engine() {
        let close = |(a, b): (f64, f64), (c, d): (f64, f64)| (a - c).abs() < 1e-9 && (b - d).abs() < 1e-9;
        let speed = |rate, preserve_pitch, transpose| Speed { rate, preserve_pitch, transpose };
        assert!(close(speed(80.0, false, 0.0).factors(), (0.8, 0.8)));
        assert!(close(speed(80.0, true, 0.0).factors(), (0.8, 1.0)));
        assert!(close(speed(100.0, true, 3.0).factors(), (1.0, 2f64.powf(0.25))));
        assert!(close(speed(50.0, false, 12.0).factors(), (0.5, 1.0)));
        // Out of range: clamped.
        assert!(close(speed(300.0, true, -20.0).factors(), (1.5, 0.5)));
        assert!(close(speed(f64::NAN, true, f64::NAN).factors(), (1.0, 1.0)));
        assert!(speed(300.0, true, 0.0).out_of_range());
        assert!(!Speed::default().out_of_range());
    }

    #[test]
    fn rate_cue_converts_with_targets_duration_and_curve() {
        let pan = CueKind::Pan(PanCue {
            targets: Targets::All(AllKeyword::All),
            pan: 0.5,
            duration: 3.0,
            curve: PanCurve::Linear,
        });
        let Some(CueKind::Rate(r)) = pan.convert(ControlKind::Rate) else { panic!() };
        assert_eq!(r.targets, Targets::All(AllKeyword::All));
        assert_eq!((r.duration, r.curve, r.rate, r.transpose), (3.0, PanCurve::Linear, Some(100.0), None));
        let Some(CueKind::Volume(v)) = CueKind::Rate(r).convert(ControlKind::Volume) else { panic!() };
        assert_eq!((v.duration, v.curve), (Some(3.0), FadeCurve::Linear));
    }
}
