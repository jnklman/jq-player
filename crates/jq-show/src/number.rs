//! Cue numbers for newly inserted cues.

use std::collections::HashSet;

use crate::model::Cue;

/// Numbers for `count` cues inserted at `at` into `list`, whose numbers are
/// `prefix` followed by a decimal (`""` in the main list, `"7."` in group 7).
///
/// Between n and m (the nearest numbered cues around `at`; n = 0 at the
/// start, no m at the end) the cues get n+1, n+2, … (next whole numbers)
/// if they fit below m, otherwise evenly spaced decimals: one cue between
/// 2 and 3 gets 2.5, two get 2.3 and 2.7. Numbers in `taken` are avoided;
/// if nothing fits, the cues get the largest whole number in the list + 1, + 2, …
pub fn new_numbers(
    list: &[Cue],
    prefix: &str,
    at: usize,
    count: usize,
    taken: &HashSet<&str>,
) -> Vec<String> {
    let value = |c: &Cue| c.number.strip_prefix(prefix).and_then(parse);
    let at = at.min(list.len());
    let prev = list[..at].iter().rev().find_map(value).unwrap_or((0.0, 0));
    let next = list[at..].iter().find_map(value);
    let numbers =
        between(prev, next, count).map(|v| v.into_iter().map(|n| format!("{prefix}{n}")).collect::<Vec<_>>());
    if let Some(numbers) = numbers
        && numbers.iter().all(|n| !taken.contains(n.as_str()))
    {
        return numbers;
    }
    let max = list.iter().filter_map(value).map(|(v, _)| v.floor() as u64).max().unwrap_or(0);
    (max + 1..).map(|n| format!("{prefix}{n}")).filter(|n| !taken.contains(n.as_str())).take(count).collect()
}

/// `count` ascending numbers strictly between `prev` and `next` (values with
/// their number of decimals), or `None` if `next` is not above `prev`.
fn between(prev: (f64, usize), next: Option<(f64, usize)>, count: usize) -> Option<Vec<String>> {
    let (n, n_decimals) = prev;
    let whole = n.floor() + 1.0;
    let Some((m, m_decimals)) = next else {
        return Some((0..count).map(|i| format_decimal(whole + i as f64, 0)).collect());
    };
    if m <= n {
        return None;
    }
    if whole + count as f64 - 1.0 < m {
        return Some((0..count).map(|i| format_decimal(whole + i as f64, 0)).collect());
    }
    // Fewest decimals (at least one more than n and m have) that keep the
    // evenly spaced values distinct and between n and m.
    for decimals in n_decimals.max(m_decimals) + 1..=9 {
        let scale = 10f64.powi(decimals as i32);
        let values: Vec<f64> = (1..=count)
            .map(|i| n + (m - n) * i as f64 / (count + 1) as f64)
            .map(|v| (v * scale).round() / scale)
            .collect();
        let ascending = values.windows(2).all(|w| w[0] < w[1]);
        if ascending && values.first().is_some_and(|&v| v > n) && values.last().is_some_and(|&v| v < m) {
            return Some(values.into_iter().map(|v| format_decimal(v, decimals)).collect());
        }
    }
    None
}

/// Parses `12` or `12.5` into the value and its number of decimals.
fn parse(s: &str) -> Option<(f64, usize)> {
    let (whole, fraction) = s.split_once('.').unwrap_or((s, ""));
    let digits = |p: &str| p.bytes().all(|b| b.is_ascii_digit());
    if whole.is_empty() || !digits(whole) || !digits(fraction) || s.ends_with('.') {
        return None;
    }
    Some((s.parse().ok()?, fraction.len()))
}

/// `value` with up to `decimals` decimals, without trailing zeros.
fn format_decimal(value: f64, decimals: usize) -> String {
    let s = format!("{value:.decimals$}");
    if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CueKind, WaitCue};

    fn list(numbers: &[&str]) -> Vec<Cue> {
        numbers
            .iter()
            .map(|n| {
                let mut c = Cue::new(CueKind::Wait(WaitCue { duration: 1.0 }));
                c.number = (*n).into();
                c
            })
            .collect()
    }

    fn numbers(existing: &[&str], at: usize, count: usize) -> Vec<String> {
        let cues = list(existing);
        let taken = cues.iter().map(|c| c.number.as_str()).collect();
        new_numbers(&cues, "", at, count, &taken)
    }

    #[test]
    fn whole_number_if_there_is_room() {
        assert_eq!(numbers(&["1", "5"], 1, 1), ["2"]);
        assert_eq!(numbers(&["1", "5"], 1, 3), ["2", "3", "4"]);
        assert_eq!(numbers(&["1", "2"], 2, 2), ["3", "4"], "at the end");
        assert_eq!(numbers(&[], 0, 2), ["1", "2"]);
        assert_eq!(numbers(&["2.5", "5"], 1, 1), ["3"], "next whole number after a decimal");
    }

    #[test]
    fn halves_the_gap_if_no_whole_number_fits() {
        assert_eq!(numbers(&["2", "3"], 1, 1), ["2.5"]);
        assert_eq!(numbers(&["2.5", "3"], 1, 1), ["2.75"]);
        assert_eq!(numbers(&["2", "2.1"], 1, 1), ["2.05"]);
        assert_eq!(numbers(&["1"], 0, 1), ["0.5"], "at the start");
    }

    #[test]
    fn several_cues_share_the_gap() {
        assert_eq!(numbers(&["2", "3"], 1, 2), ["2.3", "2.7"]);
        assert_eq!(numbers(&["2", "3"], 1, 3), ["2.3", "2.5", "2.8"]);
        assert_eq!(numbers(&["1", "3"], 1, 3), ["1.5", "2", "2.5"], "two whole numbers are not enough");
        assert_eq!(numbers(&["2", "2.1"], 1, 2), ["2.03", "2.07"]);
    }

    #[test]
    fn skips_cues_without_a_number() {
        assert_eq!(numbers(&["2", "", "Pause", "3"], 2, 1), ["2.5"]);
    }

    #[test]
    fn falls_back_to_the_end_if_out_of_order() {
        assert_eq!(numbers(&["5", "3"], 1, 1), ["6"]);
        assert_eq!(numbers(&["1", "3", "2"], 1, 1), ["4"], "2 is taken");
    }

    #[test]
    fn group_numbers_keep_the_prefix() {
        let cues = list(&["7.1", "7.2"]);
        let taken = HashSet::new();
        assert_eq!(new_numbers(&cues, "7.", 1, 1, &taken), ["7.1.5"]);
        assert_eq!(new_numbers(&cues, "7.", 2, 1, &taken), ["7.3"]);
    }
}
