//! Golden tests for the ChordPro parser.
//!
//! This integration test discovers all subdirectories under `tests/fixtures/`,
//! parses each `input.cho` file through [`chordpro_core::parse`], and compares
//! the pretty-printed Debug output (`{:#?}`) of the resulting AST against the
//! corresponding `expected.txt` file.
//!
//! # Adding a new golden test
//!
//! 1. Create a new subdirectory under `crates/core/tests/fixtures/` with a
//!    descriptive kebab-case name (e.g., `section-directives`).
//! 2. Add an `input.cho` file containing the ChordPro source to parse.
//! 3. Run `UPDATE_GOLDEN=1 cargo test -p chordpro-core --test golden` to
//!    automatically generate the `expected.txt` file from the current parser
//!    output.
//! 4. Review the generated `expected.txt` to confirm it matches the intended
//!    behavior, then commit both files.
//!
//! # Updating golden snapshots
//!
//! When the parser output changes intentionally (e.g., new AST fields), run:
//!
//! ```sh
//! UPDATE_GOLDEN=1 cargo test -p chordpro-core --test golden
//! ```
//!
//! This overwrites every `expected.txt` with the current parser output. Review
//! the diffs carefully before committing.

use std::fs;
use std::path::{Path, PathBuf};

/// Returns the path to the `tests/fixtures/` directory.
fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// Collects all fixture subdirectories (each must contain `input.cho`).
fn discover_fixtures() -> Vec<PathBuf> {
    let dir = fixtures_dir();
    let mut fixtures: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read fixtures directory {}: {}", dir.display(), e))
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_dir() && path.join("input.cho").exists() {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    // Sort for deterministic test ordering.
    fixtures.sort();
    fixtures
}

/// Produces a unified-diff-style comparison between two strings.
///
/// Returns `None` if the strings are equal, or `Some(diff)` with a
/// human-readable diff showing context around each mismatch.
fn diff_strings(expected: &str, actual: &str) -> Option<String> {
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();

    if expected_lines == actual_lines {
        return None;
    }

    let mut output = String::new();
    let max_len = expected_lines.len().max(actual_lines.len());
    let context = 3;
    let mut last_printed = None;

    for i in 0..max_len {
        let exp = expected_lines.get(i).copied();
        let act = actual_lines.get(i).copied();

        if exp != act {
            // Print context lines before the mismatch.
            let start = if i >= context { i - context } else { 0 };
            for j in start..i {
                if last_printed.is_none() || last_printed.unwrap() < j {
                    output.push_str(&format!("  {:4} | {}\n", j + 1, expected_lines[j]));
                }
            }

            // Print the differing lines.
            match (exp, act) {
                (Some(e), Some(a)) => {
                    output.push_str(&format!("- {:4} | {}\n", i + 1, e));
                    output.push_str(&format!("+ {:4} | {}\n", i + 1, a));
                }
                (Some(e), None) => {
                    output.push_str(&format!("- {:4} | {}\n", i + 1, e));
                }
                (None, Some(a)) => {
                    output.push_str(&format!("+ {:4} | {}\n", i + 1, a));
                }
                (None, None) => unreachable!(),
            }

            // Print context lines after the mismatch.
            let end = (i + context + 1).min(max_len);
            last_printed = Some(i);
            for j in (i + 1)..end {
                if let (Some(e), Some(a)) = (expected_lines.get(j), actual_lines.get(j)) {
                    if e == a {
                        output.push_str(&format!("  {:4} | {}\n", j + 1, e));
                        last_printed = Some(j);
                    }
                }
            }
        }
    }

    Some(output)
}

/// Runs a single golden test for the given fixture directory.
fn run_golden_test(fixture_dir: &Path) {
    let name = fixture_dir
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();

    let input_path = fixture_dir.join("input.cho");
    let expected_path = fixture_dir.join("expected.txt");

    let input = fs::read_to_string(&input_path)
        .unwrap_or_else(|e| panic!("[{name}] cannot read {}: {e}", input_path.display()));

    let song = chordpro_core::parse(&input).unwrap_or_else(|e| panic!("[{name}] parse error: {e}"));

    let actual = format!("{:#?}\n", song);

    // If UPDATE_GOLDEN is set, write the actual output as the new expected file.
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        fs::write(&expected_path, &actual)
            .unwrap_or_else(|e| panic!("[{name}] cannot write {}: {e}", expected_path.display()));
        eprintln!("[{name}] updated {}", expected_path.display());
        return;
    }

    let expected = fs::read_to_string(&expected_path).unwrap_or_else(|e| {
        panic!(
            "[{name}] cannot read {} (run `UPDATE_GOLDEN=1 cargo test -p chordpro-core --test golden` to create it): {e}",
            expected_path.display()
        )
    });

    if let Some(diff) = diff_strings(&expected, &actual) {
        panic!(
            "\n\ngolden test '{name}' failed!\n\
             \n\
             Fixture: {}\n\
             Expected file: {}\n\
             \n\
             Diff (- expected, + actual):\n\
             {diff}\n\
             \n\
             If this change is intentional, run:\n\
             UPDATE_GOLDEN=1 cargo test -p chordpro-core --test golden\n",
            fixture_dir.display(),
            expected_path.display(),
        );
    }
}

#[test]
fn golden_tests() {
    let fixtures = discover_fixtures();
    assert!(
        !fixtures.is_empty(),
        "no golden test fixtures found in {}",
        fixtures_dir().display()
    );

    let mut failures = Vec::new();

    for fixture in &fixtures {
        let name = fixture.file_name().unwrap().to_string_lossy().to_string();

        // Use std::panic::catch_unwind to collect all failures rather than
        // stopping at the first one.
        let result = std::panic::catch_unwind(|| {
            run_golden_test(fixture);
        });

        if let Err(e) = result {
            let msg = if let Some(s) = e.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = e.downcast_ref::<&str>() {
                (*s).to_string()
            } else {
                "unknown panic".to_string()
            };
            failures.push((name, msg));
        }
    }

    if !failures.is_empty() {
        let mut report = format!("\n{} golden test(s) failed:\n", failures.len());
        for (name, msg) in &failures {
            report.push_str(&format!("\n--- {name} ---\n{msg}\n"));
        }
        panic!("{report}");
    }

    eprintln!("all {} golden test(s) passed", fixtures.len());
}
