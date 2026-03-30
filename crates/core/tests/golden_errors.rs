//! Golden tests for parse error diagnostics.
//!
//! This integration test discovers all subdirectories under
//! `tests/error_fixtures/`, parses each `input.cho` file through
//! [`chordpro_core::parse`], and verifies that parsing fails with an error
//! whose `Display` output matches the content of `expected_error.txt`.
//!
//! # Adding a new error golden test
//!
//! 1. Create a new subdirectory under `crates/core/tests/error_fixtures/` with
//!    a descriptive kebab-case name (e.g., `unclosed-chord`).
//! 2. Add an `input.cho` file containing the malformed ChordPro source.
//! 3. Run `UPDATE_GOLDEN=1 cargo test -p chordpro-core --test golden_errors`
//!    to generate `expected_error.txt` from the current error output.
//! 4. Review the generated file, then commit both files.
//!
//! # Updating error golden snapshots
//!
//! ```sh
//! UPDATE_GOLDEN=1 cargo test -p chordpro-core --test golden_errors
//! ```

use std::fs;
use std::path::{Path, PathBuf};

/// Returns the path to the `tests/error_fixtures/` directory.
fn error_fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("error_fixtures")
}

/// Collects all fixture subdirectories (each must contain `input.cho`).
fn discover_error_fixtures() -> Vec<PathBuf> {
    let dir = error_fixtures_dir();
    let mut fixtures: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| {
            panic!(
                "cannot read error_fixtures directory {}: {}",
                dir.display(),
                e
            )
        })
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

    fixtures.sort();
    fixtures
}

/// Runs a single error golden test for the given fixture directory.
fn run_error_golden_test(fixture_dir: &Path) {
    let name = fixture_dir
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();

    let input_path = fixture_dir.join("input.cho");
    let expected_path = fixture_dir.join("expected_error.txt");

    let input = fs::read_to_string(&input_path)
        .unwrap_or_else(|e| panic!("[{name}] cannot read {}: {e}", input_path.display()));

    let err = chordpro_core::parse(&input).expect_err(&format!(
        "[{name}] expected parse error but parsing succeeded"
    ));

    let actual = format!("{err}\n");

    // If UPDATE_GOLDEN is set, write the actual error as the new expected file.
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        fs::write(&expected_path, &actual)
            .unwrap_or_else(|e| panic!("[{name}] cannot write {}: {e}", expected_path.display()));
        eprintln!("[{name}] updated {}", expected_path.display());
        return;
    }

    let expected = fs::read_to_string(&expected_path)
        .unwrap_or_else(|e| {
            panic!(
                "[{name}] cannot read {} (run `UPDATE_GOLDEN=1 cargo test -p chordpro-core --test golden_errors` to create it): {e}",
                expected_path.display()
            )
        })
        .replace("\r\n", "\n");

    assert_eq!(
        actual,
        expected,
        "\n\nerror golden test '{name}' failed!\n\
         \n\
         Fixture: {}\n\
         Expected: {}\n\
         \n\
         Expected error:\n  {}\n\
         Actual error:\n  {}\n\
         \n\
         If this change is intentional, run:\n\
         UPDATE_GOLDEN=1 cargo test -p chordpro-core --test golden_errors\n",
        fixture_dir.display(),
        expected_path.display(),
        expected.trim(),
        actual.trim(),
    );
}

#[test]
fn error_golden_tests() {
    let fixtures = discover_error_fixtures();
    assert!(
        !fixtures.is_empty(),
        "no error golden test fixtures found in {}",
        error_fixtures_dir().display()
    );

    let mut failures = Vec::new();

    for fixture in &fixtures {
        let name = fixture.file_name().unwrap().to_string_lossy().to_string();

        let result = std::panic::catch_unwind(|| {
            run_error_golden_test(fixture);
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
        let mut report = format!("\n{} error golden test(s) failed:\n", failures.len());
        for (name, msg) in &failures {
            report.push_str(&format!("\n--- {name} ---\n{msg}\n"));
        }
        panic!("{report}");
    }

    eprintln!("all {} error golden test(s) passed", fixtures.len());
}
