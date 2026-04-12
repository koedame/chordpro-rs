//! Golden test for multi-song file parsing via `parse_multi`.
//!
//! This integration test verifies that `parse_multi` correctly splits a
//! multi-song `.cho` file at `{new_song}` boundaries and parses each song
//! independently.

use std::fs;
use std::path::{Path, PathBuf};

/// Returns the path to the `tests/fixtures/` directory.
fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

#[test]
fn multi_song_golden_test() {
    let fixture_dir = fixtures_dir().join("multi-song");
    let input_path = fixture_dir.join("input.cho");
    let expected_path = fixture_dir.join("expected_multi.txt");

    let input = fs::read_to_string(&input_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", input_path.display()));

    let songs =
        chordsketch_core::parse_multi(&input).unwrap_or_else(|e| panic!("parse_multi error: {e}"));

    let actual = format!("{:#?}\n", songs);

    // If UPDATE_GOLDEN is set, write the actual output as the new expected file.
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        fs::write(&expected_path, &actual)
            .unwrap_or_else(|e| panic!("cannot write {}: {e}", expected_path.display()));
        eprintln!("updated {}", expected_path.display());
        return;
    }

    let expected = fs::read_to_string(&expected_path)
        .unwrap_or_else(|e| {
            panic!(
                "cannot read {} (run `UPDATE_GOLDEN=1 cargo test -p chordsketch-core --test golden_multi_song` to create it): {e}",
                expected_path.display()
            )
        })
        .replace("\r\n", "\n");

    assert_eq!(
        expected, actual,
        "multi-song golden test failed! Run `UPDATE_GOLDEN=1 cargo test -p chordsketch-core --test golden_multi_song` to update."
    );
}

/// Golden test: `{new_song}` appearing as the very first line produces an empty
/// first segment and the remainder is parsed as the second song.
#[test]
fn new_song_at_start_golden_test() {
    let fixture_dir = fixtures_dir().join("new-song-at-start");
    let input_path = fixture_dir.join("input.cho");
    let expected_path = fixture_dir.join("expected_multi.txt");

    let input = fs::read_to_string(&input_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", input_path.display()));

    let songs =
        chordsketch_core::parse_multi(&input).unwrap_or_else(|e| panic!("parse_multi error: {e}"));

    let actual = format!("{:#?}\n", songs);

    // If UPDATE_GOLDEN is set, write the actual output as the new expected file.
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        fs::write(&expected_path, &actual)
            .unwrap_or_else(|e| panic!("cannot write {}: {e}", expected_path.display()));
        eprintln!("updated {}", expected_path.display());
        return;
    }

    let expected = fs::read_to_string(&expected_path)
        .unwrap_or_else(|e| {
            panic!(
                "cannot read {} (run `UPDATE_GOLDEN=1 cargo test -p chordsketch-core --test golden_multi_song` to create it): {e}",
                expected_path.display()
            )
        })
        .replace("\r\n", "\n");

    assert_eq!(
        expected, actual,
        "new-song-at-start golden test failed! Run `UPDATE_GOLDEN=1 cargo test -p chordsketch-core --test golden_multi_song` to update."
    );
}
