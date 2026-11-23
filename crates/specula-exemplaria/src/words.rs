//! Names: what a name may be, and the nearest names to a word that names nothing (the engine's
//! index across the shape library, this library and a project uses the same measure).

/// The longest name of a kind, a plan, a number, a trait, a role, a clip or an option.
pub const MAX_NAME: usize = 48;
/// The longest word [`nearest`] measures (longer words are cut, so the work is bounded).
pub const MAX_WORD: usize = 64;

/// A name: a lowercase letter, then lowercase letters, digits and `_`, at most [`MAX_NAME`].
pub fn valid_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty() && b.len() <= MAX_NAME && b[0].is_ascii_lowercase() && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
}

/// Edit distance (insertions, deletions, substitutions), both words cut to [`MAX_WORD`] bytes.
pub fn distance(a: &str, b: &str) -> usize {
    let a = &a.as_bytes()[..a.len().min(MAX_WORD)];
    let b = &b.as_bytes()[..b.len().min(MAX_WORD)];
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut row = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != cb);
            row[j + 1] = sub.min(prev[j + 1] + 1).min(row[j] + 1);
        }
        std::mem::swap(&mut prev, &mut row);
    }
    prev[b.len()]
}

/// How near `name` is to `word` (lower is nearer), or `None` when it is not near at all: a name
/// that holds the word, or the word the name (`wolf` and `grey_wolf`), counts as one edit; any
/// other within a third of the longer word's length (at least two edits).
pub fn nearness(word: &str, name: &str) -> Option<usize> {
    // (Cut to MAX_WORD bytes, on a character's boundary: the work is bounded by it.)
    let mut cut = word.len().min(MAX_WORD);
    while !word.is_char_boundary(cut) {
        cut -= 1;
    }
    let word = word[..cut].to_ascii_lowercase();
    if word.is_empty() {
        return None;
    }
    if name == word {
        return Some(0);
    }
    if word.len() >= 3 && (name.contains(&word) || word.contains(name)) {
        return Some(1);
    }
    let d = distance(&word, name);
    (d <= (word.len().max(name.len()) / 3).max(2)).then_some(d)
}

/// Up to `n` of `names` nearest to `word`, nearest first (ties by name), each once.
pub fn nearest<'a>(word: &str, names: impl IntoIterator<Item = &'a str>, n: usize) -> Vec<String> {
    let mut scored: Vec<(usize, &str)> = names.into_iter().filter_map(|name| nearness(word, name).map(|d| (d, name))).collect();
    scored.sort();
    scored.dedup_by(|a, b| a.1 == b.1);
    scored.into_iter().take(n).map(|(_, name)| name.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_nearness() {
        assert!(valid_name("grey_wolf") && valid_name("a1") && !valid_name("Wolf") && !valid_name("1wolf") && !valid_name("") && !valid_name("wolf-dog"));
        assert!(!valid_name(&"a".repeat(MAX_NAME + 1)));
        assert_eq!(distance("kitten", "sitting"), 3);
        assert_eq!(nearest("box_creatur", ["box_creature", "gear", "torus"], 5), ["box_creature"]);
        assert_eq!(nearest("wolf", ["grey_wolf", "dire_wolf", "cube"], 5), ["dire_wolf", "grey_wolf"]);
        assert!(nearest("zzzzzz", ["box_creature"], 5).is_empty());
        // A word of 1 MB costs no more than one of MAX_WORD bytes.
        assert!(nearest(&"x".repeat(1 << 20), ["box_creature"], 5).is_empty());
    }
}
