pub fn subsequence_score(pattern: &str, target: &str) -> Option<u32> {
    let pattern_len = pattern.chars().count();
    if pattern_len == 0 {
        return Some(0);
    }
    let target_len = target.chars().count();
    if target_len < pattern_len {
        return None;
    }

    let mut pattern_chars = pattern.chars();
    let mut current_pattern_char = pattern_chars.next();
    let mut prev_pattern_char: Option<char> = None;
    let mut prev_target_char: Option<char> = None;
    let mut matched = 0usize;
    let mut score = 0u32;
    let mut start_match = false;
    let mut remaining = target_len;

    for target_char in target.chars() {
        remaining -= 1;

        if let Some(pattern_char) = current_pattern_char {
            if target_char == pattern_char {
                start_match = true;
                score += 10;
                if prev_target_char.is_some() && prev_target_char == prev_pattern_char {
                    score += 10;
                }
                if is_word_boundary(prev_target_char) {
                    score += 15;
                }
                prev_pattern_char = Some(pattern_char);
                matched += 1;
                current_pattern_char = pattern_chars.next();
                if current_pattern_char.is_none() {
                    return Some(score);
                }
            } else if start_match {
                score = score.saturating_sub(1);
            }
        }

        prev_target_char = Some(target_char);

        if remaining < pattern_len - matched {
            return None;
        }
    }

    None
}

pub fn normalize_by_length(score: u32, field_len: usize) -> u32 {
    score * 100 / (field_len as u32 + 8)
}

fn is_word_boundary(prev: Option<char>) -> bool {
    match prev {
        None => true,
        Some(c) => matches!(c, ' ' | '-' | '_' | '(' | ')' | '.' | '/' | '\''),
    }
}
