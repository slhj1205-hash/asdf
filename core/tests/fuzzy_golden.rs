#![allow(clippy::unwrap_used)]

#[allow(dead_code)]
mod fixtures;

use lyre_core::{FuzzyQuery, Song, fuzzy::subsequence_score};

#[test]
fn subsequence_scores_are_unchanged() {
    let expected = [
        ("blue", "blue", 368),
        ("blue", "blue moon", 233),
        ("blue", "electric blue", 186),
        ("blue", "blue monday", 231),
        ("b", "blue sky", 126),
        ("b", "ruby sky", 6),
        ("ab", "a---ab", 130),
        ("ab", "a---b", 78),
        ("btl", "beetle", 86),
        ("cafe", "Café", 368),
        ("acdc", "AC/DC", 148),
        ("dont", "Don't Stop", 233),
        ("the", "The Beatles", 195),
        ("beat", "The Beatles", 133),
        ("tb", "The Beatles", 68),
    ];

    for (pattern, target, score) in expected {
        assert_eq!(
            subsequence_score(pattern, target),
            Some(score),
            "the score for {pattern:?} against {target:?} changed"
        );
    }
}

#[test]
fn song_scores_are_unchanged() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = fixtures::write_song(
        dir.path(),
        "song.wav",
        "Blue Monday",
        "New Order",
        "Power Corruption",
    );
    let song = Song::load(&path).unwrap();

    let expected = [
        ("blue", Some(346)),
        ("new order", Some(578)),
        ("blue order", Some(577)),
        ("monday power", Some(658)),
        ("zzz", None),
    ];

    for (raw, score) in expected {
        assert_eq!(
            song.fuzzy_score(&FuzzyQuery::new(raw)),
            score,
            "the score for the query {raw:?} changed"
        );
    }
}
