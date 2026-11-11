//! Playing order of playlist cues: the list's or a shuffled one.
//!
//! A playlist counts its items in steps from its start on, through all the
//! passes of a repeating one; the order says which file each step plays.

use jq_show::{PlaylistCue, Shuffle};

/// Shuffled order of one start of a playlist.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Order {
    /// Files the playlist had when it was drawn.
    count: usize,
    /// Reshuffled every pass (else the one pass repeats).
    per_pass: bool,
    /// File per step: one pass or, reshuffled every pass, the passes drawn
    /// so far.
    files: Vec<usize>,
}

impl Order {
    /// The order a start of `list` plays in; `None` = the list's.
    pub fn draw(list: &PlaylistCue, rng: &mut fastrand::Rng) -> Option<Self> {
        let per_pass = match list.shuffle {
            Shuffle::Off => return None,
            Shuffle::PerStart => false,
            Shuffle::PerPass => list.repeat,
        };
        let mut order = Self { count: list.items.len(), per_pass, files: Vec::new() };
        order.add_pass(rng);
        Some(order)
    }

    /// Drawn for `list` as it is now (an edit of its files or shuffle setting
    /// makes it stale).
    pub fn fits(&self, list: &PlaylistCue) -> bool {
        let per_pass = list.shuffle == Shuffle::PerPass && list.repeat;
        !list.shuffle.is_off() && self.count == list.items.len() && self.per_pass == per_pass
    }

    /// Draws further passes until `step` is known.
    pub fn reach(&mut self, step: usize, rng: &mut fastrand::Rng) {
        while self.per_pass && self.count > 0 && self.files.len() <= step {
            self.add_pass(rng);
        }
    }

    fn add_pass(&mut self, rng: &mut fastrand::Rng) {
        let mut pass: Vec<usize> = (0..self.count).collect();
        rng.shuffle(&mut pass);
        // A pass does not start with the item just played.
        if self.count > 1 && self.files.last().is_some_and(|last| pass.first() == Some(last)) {
            pass.swap(0, rng.usize(1..self.count));
        }
        self.files.extend(pass);
    }

    fn file(&self, step: usize) -> Option<usize> {
        match self.per_pass {
            true => self.files.get(step).copied(),
            false => self.files.get(step % self.count.max(1)).copied(),
        }
    }
}

/// The file step `step` of a playlist with `count` files plays, in `order`
/// (`None`, or one drawn for another count: the list's). `None` without files.
pub(crate) fn step_file(order: Option<&Order>, step: usize, count: usize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let shuffled = order.filter(|order| order.count == count).and_then(|order| order.file(step));
    Some(shuffled.unwrap_or(step % count))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(count: usize, shuffle: Shuffle, repeat: bool) -> PlaylistCue {
        PlaylistCue { shuffle, repeat, ..PlaylistCue::new((0..count).map(|i| format!("{i}.wav")).collect()) }
    }

    fn pass(order: &Order, pass: usize, count: usize) -> Vec<usize> {
        (pass * count..(pass + 1) * count).map(|step| step_file(Some(order), step, count).unwrap()).collect()
    }

    fn sorted(mut files: Vec<usize>) -> Vec<usize> {
        files.sort_unstable();
        files
    }

    #[test]
    fn off_plays_the_list_order() {
        let mut rng = fastrand::Rng::with_seed(1);
        assert_eq!(Order::draw(&list(3, Shuffle::Off, true), &mut rng), None);
        let steps: Vec<_> = (0..5).map(|step| step_file(None, step, 3).unwrap()).collect();
        assert_eq!(steps, [0, 1, 2, 0, 1]);
        assert_eq!(step_file(None, 0, 0), None);
    }

    #[test]
    fn per_start_repeats_one_pass() {
        let mut rng = fastrand::Rng::with_seed(1);
        let shuffled = list(8, Shuffle::PerStart, true);
        let mut order = Order::draw(&shuffled, &mut rng).unwrap();
        order.reach(40, &mut rng);
        let first = pass(&order, 0, 8);
        assert_eq!(sorted(first.clone()), (0..8).collect::<Vec<_>>(), "every item once");
        assert_ne!(first, (0..8).collect::<Vec<_>>(), "shuffled");
        assert_eq!(pass(&order, 1, 8), first);
        assert_eq!(pass(&order, 4, 8), first);
    }

    #[test]
    fn per_pass_reshuffles_without_playing_an_item_twice_in_a_row() {
        let mut rng = fastrand::Rng::with_seed(7);
        let shuffled = list(8, Shuffle::PerPass, true);
        let mut order = Order::draw(&shuffled, &mut rng).unwrap();
        order.reach(8 * 50, &mut rng);
        let passes: Vec<_> = (0..50).map(|number| pass(&order, number, 8)).collect();
        for files in &passes {
            assert_eq!(sorted(files.clone()), (0..8).collect::<Vec<_>>());
        }
        assert!(passes.windows(2).any(|pair| pair[0] != pair[1]), "reshuffled");
        for pair in passes.windows(2) {
            assert_ne!(pair[0].last(), pair[1].first());
        }
        // Two items: the passes alternate.
        let mut two = Order::draw(&list(2, Shuffle::PerPass, true), &mut rng).unwrap();
        two.reach(20, &mut rng);
        for step in 1..20 {
            assert_ne!(step_file(Some(&two), step, 2), step_file(Some(&two), step - 1, 2));
        }
    }

    #[test]
    fn a_stale_order_is_recognized_and_falls_back() {
        let mut rng = fastrand::Rng::with_seed(3);
        let order = Order::draw(&list(4, Shuffle::PerPass, true), &mut rng).unwrap();
        assert!(order.fits(&list(4, Shuffle::PerPass, true)));
        assert!(!order.fits(&list(5, Shuffle::PerPass, true)));
        assert!(!order.fits(&list(4, Shuffle::PerStart, true)));
        assert!(!order.fits(&list(4, Shuffle::Off, true)));
        assert!(Order::draw(&list(4, Shuffle::PerPass, false), &mut rng).unwrap().fits(&list(
            4,
            Shuffle::PerStart,
            false
        )));
        assert_eq!(step_file(Some(&order), 2, 3), Some(2), "drawn for another count: the list's");
    }
}
