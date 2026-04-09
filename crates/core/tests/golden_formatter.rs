//! Golden tests for the ChordPro source formatter.
//!
//! Each subdirectory under `tests/fixtures/` that contains both `input.cho`
//! and `expected_formatted.cho` is run as a formatter test. The formatter is
//! applied to the input and the result is compared against the expected
//! output file.
//!
//! # Adding a new golden formatter test
//!
//! 1. Create a new subdirectory under `crates/core/tests/fixtures/` (or reuse
//!    an existing one that already has an `input.cho`).
//! 2. Add an `expected_formatted.cho` file containing the expected formatted
//!    output.
//! 3. Run `cargo test -p chordsketch-core --test golden_formatter` to verify.

use std::fs;
use std::path::{Path, PathBuf};

/// Returns the path to the `tests/fixtures/` directory.
fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

#[test]
fn formatter_golden_tests() {
    let fixtures = fixtures_dir();
    let mut tested = 0;
    let mut failed = 0;

    let mut entries: Vec<_> = fs::read_dir(&fixtures)
        .expect("fixtures directory missing")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.path())
        .collect();
    entries.sort();

    for dir in entries {
        let input_path = dir.join("input.cho");
        let expected_path = dir.join("expected_formatted.cho");

        if !input_path.exists() || !expected_path.exists() {
            continue;
        }

        let input = fs::read_to_string(&input_path).unwrap_or_else(|e| {
            panic!("failed to read {}: {e}", input_path.display())
        });
        let expected = fs::read_to_string(&expected_path).unwrap_or_else(|e| {
            panic!("failed to read {}: {e}", expected_path.display())
        });

        let actual = chordsketch_core::formatter::format(
            &input,
            &chordsketch_core::formatter::FormatOptions::default(),
        );

        if actual != expected {
            eprintln!("FAIL: {}", dir.display());
            eprintln!("  Expected:\n{expected}");
            eprintln!("  Got:\n{actual}");
            failed += 1;
        } else {
            tested += 1;
        }
    }

    assert_eq!(
        failed,
        0,
        "{failed} formatter golden test(s) failed (see above)"
    );
    assert!(tested > 0, "no formatter golden tests were found");
}
