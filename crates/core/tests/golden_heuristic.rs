//! Golden tests for the plain-text chord+lyrics heuristic importer.
//!
//! Each subdirectory under `tests/fixtures/` that contains an `input.txt` file
//! is treated as a heuristic importer test case.  The `input.txt` is converted
//! with [`chordsketch_core::convert_plain_text`] and serialized with
//! [`chordsketch_core::song_to_chordpro`]; the result is compared against the
//! corresponding `expected.cho` file.
//!
//! # Adding a new fixture
//!
//! 1. Create a subdirectory under `crates/core/tests/fixtures/` with a
//!    descriptive kebab-case name (e.g., `heuristic-my-case`).
//! 2. Add an `input.txt` containing the plain-text chord sheet.
//! 3. Run `UPDATE_GOLDEN=1 cargo test -p chordsketch-core --test golden_heuristic`
//!    to generate `expected.cho` from the current converter output.
//! 4. Review `expected.cho` and commit both files.

use std::fs;
use std::path::{Path, PathBuf};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

fn discover_fixtures() -> Vec<PathBuf> {
    let dir = fixtures_dir();
    let mut fixtures: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read fixtures directory {}: {}", dir.display(), e))
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_dir() && path.join("input.txt").exists() {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    fixtures.sort();
    fixtures
}

#[test]
fn heuristic_golden_tests() {
    let update = std::env::var("UPDATE_GOLDEN").is_ok();
    let fixtures = discover_fixtures();
    assert!(
        !fixtures.is_empty(),
        "no heuristic fixtures found (expected directories with input.txt)"
    );

    let mut failed = false;

    for fixture in &fixtures {
        let name = fixture.file_name().unwrap().to_string_lossy();
        let input_path = fixture.join("input.txt");
        let expected_path = fixture.join("expected.cho");

        let input = fs::read_to_string(&input_path).unwrap_or_else(|e| {
            panic!("{name}: cannot read input.txt: {e}");
        });

        let song = chordsketch_core::convert_plain_text(&input);
        let actual = chordsketch_core::song_to_chordpro(&song);

        if update {
            fs::write(&expected_path, &actual).unwrap_or_else(|e| {
                panic!("{name}: cannot write expected.cho: {e}");
            });
            println!("updated: {}", expected_path.display());
        } else {
            let expected = fs::read_to_string(&expected_path).unwrap_or_else(|e| {
                panic!(
                    "{name}: cannot read expected.cho: {e}\n\
                     Run `UPDATE_GOLDEN=1 cargo test -p chordsketch-core --test golden_heuristic` \
                     to generate it."
                );
            });
            // Normalize CRLF → LF before comparing so that the test passes on
            // Windows even when git checks out expected.cho with \r\n endings.
            let expected_norm = expected.replace("\r\n", "\n");
            let actual_norm = actual.replace("\r\n", "\n");
            if actual_norm != expected_norm {
                eprintln!(
                    "FAIL: {name}\n--- expected ---\n{expected_norm}\n--- actual ---\n{actual_norm}"
                );
                failed = true;
            }
        }
    }

    assert!(
        !failed,
        "one or more heuristic golden tests failed (see above)"
    );
}
