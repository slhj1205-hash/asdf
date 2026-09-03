use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

const MATCH: i32 = 16;
const CONSECUTIVE: i32 = 20;
const WORD_BOUNDARY: i32 = 24;
const FIELD_START: i32 = 28;
const GAP_START: i32 = -3;
const GAP_EXTENSION: i32 = -1;
const LEADING_GAP: i32 = -1;
const LEADING_GAP_CAP: i32 = -40;

const EXACT_FIELD_BONUS: i32 = 220;
const PREFIX_BONUS: i32 = 90;
const EXACT_WORD_BONUS: i32 = 60;

const LENGTH_PENALTY: i32 = -1;
const LENGTH_PENALTY_CAP: i32 = -48;

const UNREACHABLE: i32 = i32::MIN / 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    normalized: Box<str>,
    chars: Box<[char]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuzzyQuery {
    phrase: Pattern,
    terms: Box<[Pattern]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    normalized: Box<str>,
    chars: Box<[char]>,
    boundaries: Box<[bool]>,
}

impl Pattern {
    pub fn new(raw: &str) -> Pattern {
        Pattern::from_normalized(normalize(raw))
    }

    fn from_normalized(normalized: String) -> Pattern {
        let chars: Box<[char]> = normalized.chars().collect();
        Pattern {
            normalized: normalized.into_boxed_str(),
            chars,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.normalized
    }

    pub fn chars(&self) -> &[char] {
        &self.chars
    }

    pub fn len(&self) -> usize {
        self.chars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }
}

impl FuzzyQuery {
    pub fn new(raw: &str) -> FuzzyQuery {
        let phrase = normalize(raw.trim());
        let terms: Box<[Pattern]> = phrase
            .split(' ')
            .filter(|term| !term.is_empty())
            .map(|term| Pattern::from_normalized(term.to_owned()))
            .collect();

        FuzzyQuery {
            phrase: Pattern::from_normalized(phrase),
            terms,
        }
    }

    pub fn phrase(&self) -> &Pattern {
        &self.phrase
    }

    pub fn terms(&self) -> &[Pattern] {
        &self.terms
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    pub fn is_multi_term(&self) -> bool {
        self.terms.len() > 1
    }
}

impl Candidate {
    pub fn new(raw: &str) -> Candidate {
        let normalized = normalize(raw);
        let chars: Box<[char]> = normalized.chars().collect();
        let mut boundaries = Vec::with_capacity(chars.len());
        let mut prev: Option<char> = None;
        for &ch in chars.iter() {
            let boundary = match prev {
                Some(p) => !p.is_alphanumeric() || (p.is_numeric() != ch.is_numeric()),
                None => true,
            };
            boundaries.push(boundary);
            prev = Some(ch);
        }
        Candidate {
            normalized: normalized.into_boxed_str(),
            chars,
            boundaries: boundaries.into_boxed_slice(),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.normalized
    }

    pub fn chars(&self) -> &[char] {
        &self.chars
    }

    pub fn char_len(&self) -> u32 {
        u32::try_from(self.chars.len()).unwrap_or(u32::MAX)
    }

    pub fn is_word_start(&self, index: usize) -> bool {
        self.boundaries.get(index).copied().unwrap_or(false)
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }
}

fn char_bonus(candidate: &Candidate, index: usize) -> i32 {
    if index == 0 {
        FIELD_START
    } else if candidate.is_word_start(index) {
        WORD_BOUNDARY
    } else {
        0
    }
}

#[inline]
fn is_reachable(score: i32) -> bool {
    score > UNREACHABLE
}

#[inline]
fn matchable_columns(
    row: usize,
    pattern_len: usize,
    target_len: usize,
) -> std::ops::RangeInclusive<usize> {
    row..=(target_len - (pattern_len - row))
}

fn best_alignment(pattern: &Pattern, candidate: &Candidate) -> Option<i32> {
    let pattern_chars = pattern.chars();
    let target_chars = candidate.chars();
    let pattern_len = pattern_chars.len();
    let target_len = target_chars.len();

    if pattern_len == 0 {
        return Some(0);
    }
    if target_len < pattern_len {
        return None;
    }

    let mut previous_row = vec![UNREACHABLE; target_len];
    let mut current_row = vec![UNREACHABLE; target_len];

    for (row, pattern_char) in pattern_chars.iter().enumerate() {
        let mut best_score_after_gap = UNREACHABLE;
        let mut row_has_a_match = false;

        for cell in current_row.iter_mut() {
            *cell = UNREACHABLE;
        }

        let matchable = matchable_columns(row, pattern_len, target_len);

        for column in 0..target_len {
            if column >= 1 {
                if is_reachable(best_score_after_gap) {
                    best_score_after_gap += GAP_EXTENSION;
                }
                if column >= 2 {
                    let candidate_gap =
                        previous_row.get(column - 2).copied().unwrap_or(UNREACHABLE);
                    if is_reachable(candidate_gap) {
                        best_score_after_gap =
                            best_score_after_gap.max(candidate_gap + GAP_START);
                    }
                }
            }

            if !matchable.contains(&column) {
                continue;
            }

            let target_char = match target_chars.get(column) {
                Some(value) => value,
                None => continue,
            };
            if target_char != pattern_char {
                continue;
            }

            let score_before_match = if row == 0 {
                leading_gap(column)
            } else {
                let consecutive = match column.checked_sub(1) {
                    Some(index) => previous_row.get(index).copied().unwrap_or(UNREACHABLE),
                    None => UNREACHABLE,
                };
                let consecutive = if is_reachable(consecutive) {
                    consecutive + CONSECUTIVE
                } else {
                    UNREACHABLE
                };
                consecutive.max(best_score_after_gap)
            };

            if !is_reachable(score_before_match) {
                continue;
            }

            if let Some(cell) = current_row.get_mut(column) {
                *cell = score_before_match + MATCH + char_bonus(candidate, column);
                row_has_a_match = true;
            }
        }

        if !row_has_a_match {
            return None;
        }

        std::mem::swap(&mut previous_row, &mut current_row);
    }

    previous_row.iter().copied().filter(|&v| is_reachable(v)).max()
}

pub fn subsequence_score(pattern: &str, target: &str) -> Option<u32> {
    score(&Pattern::new(pattern), &Candidate::new(target))
}

fn contains_exact_word(candidate: &Candidate, pattern: &Pattern) -> bool {
    let needle = pattern.as_str();
    if needle.is_empty() {
        return false;
    }
    candidate.as_str().split(' ').any(|word| word == needle)
}

pub fn score(pattern: &Pattern, candidate: &Candidate) -> Option<u32> {
    if pattern.is_empty() {
        return Some(0);
    }

    let mut total = best_alignment(pattern, candidate)?;

    if pattern.as_str() == candidate.as_str() {
        total += EXACT_FIELD_BONUS;
    } else if candidate.as_str().starts_with(pattern.as_str()) {
        total += PREFIX_BONUS;
    } else if contains_exact_word(candidate, pattern) {
        total += EXACT_WORD_BONUS;
    }

    total += length_penalty(candidate);

    Some(u32::try_from(total.max(0)).unwrap_or(u32::MAX))
}

fn leading_gap(index: usize) -> i32 {
    let raw = LEADING_GAP.saturating_mul(i32::try_from(index).unwrap_or(i32::MAX));
    raw.max(LEADING_GAP_CAP)
}

fn length_penalty(candidate: &Candidate) -> i32 {
    let len = i32::try_from(candidate.chars().len()).unwrap_or(i32::MAX);
    LENGTH_PENALTY.saturating_mul(len).max(LENGTH_PENALTY_CAP)
}

fn normalize(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut pending_space = false;

    for c in raw.nfd() {
        if is_combining_mark(c) {
            continue;
        }

        match c {
            '\'' | '\u{2019}' | '\u{02BC}' | '\u{2018}' | '`' => {}
            _ if c.is_whitespace() || matches!(c, '-' | '_' | '/' | '\\' | '(' | ')' | '[' | ']' | '{' | '}' | '.' | ',' | ':' | ';' | '|' | '\u{2013}' | '\u{2014}') => {
                pending_space = !out.is_empty();
            }
            _ => {
                if pending_space {
                    out.push(' ');
                    pending_space = false;
                }
                for lower in c.to_lowercase() {
                    out.push(lower);
                }
            }
        }
    }

    out.nfc().collect()
}
