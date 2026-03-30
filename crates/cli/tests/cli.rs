//! Integration tests for the `chordpro` CLI binary.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::Path;
use tempfile::NamedTempFile;

/// Returns the path to a test fixture file.
fn fixture(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
        .to_string_lossy()
        .to_string()
}

#[test]
fn test_render_to_stdout() {
    Command::cargo_bin("chordpro")
        .unwrap()
        .arg(fixture("simple.cho"))
        .assert()
        .success()
        .stdout(predicate::str::contains("Simple Song"))
        .stdout(predicate::str::contains("G     C"))
        .stdout(predicate::str::contains("Hello world"));
}

#[test]
fn test_output_to_file() {
    let output_file = NamedTempFile::new().unwrap();
    let output_path = output_file.path().to_string_lossy().to_string();

    Command::cargo_bin("chordpro")
        .unwrap()
        .args([&fixture("simple.cho"), "-o", &output_path])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    let content = std::fs::read_to_string(&output_path).unwrap();
    assert!(content.contains("Simple Song"));
    assert!(content.contains("Hello world"));
}

#[test]
fn test_nonexistent_file() {
    Command::cargo_bin("chordpro")
        .unwrap()
        .arg("/tmp/nonexistent-chordpro-test-file.cho")
        .assert()
        .failure()
        .stderr(predicate::str::contains("error:"))
        .stderr(predicate::str::contains("nonexistent"));
}

#[test]
fn test_parse_error() {
    Command::cargo_bin("chordpro")
        .unwrap()
        .arg(fixture("invalid.cho"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("error:"))
        .stderr(predicate::str::contains("parse error"));
}

#[test]
fn test_multiple_files() {
    Command::cargo_bin("chordpro")
        .unwrap()
        .args([&fixture("simple.cho"), &fixture("second.cho")])
        .assert()
        .success()
        .stdout(predicate::str::contains("Simple Song"))
        .stdout(predicate::str::contains("Second Song"));
}

#[test]
fn test_multiple_files_with_error() {
    // One valid, one invalid — should output the valid one and exit non-zero.
    Command::cargo_bin("chordpro")
        .unwrap()
        .args([&fixture("simple.cho"), &fixture("invalid.cho")])
        .assert()
        .failure()
        .stdout(predicate::str::contains("Simple Song"))
        .stderr(predicate::str::contains("error:"));
}

#[test]
fn test_version_flag() {
    Command::cargo_bin("chordpro")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("chordpro"));
}

#[test]
fn test_help_flag() {
    Command::cargo_bin("chordpro")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage"))
        .stdout(predicate::str::contains("--output"))
        .stdout(predicate::str::contains("--format"))
        .stdout(predicate::str::contains("--transpose"));
}

#[test]
fn test_transpose_up() {
    Command::cargo_bin("chordpro")
        .unwrap()
        .args([&fixture("simple.cho"), "--transpose", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("A     D"))
        .stdout(predicate::str::contains("Hello world"));
}

#[test]
fn test_transpose_down() {
    Command::cargo_bin("chordpro")
        .unwrap()
        .args([&fixture("simple.cho"), "--transpose=-2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("F     A#"))
        .stdout(predicate::str::contains("Hello world"));
}

#[test]
fn test_transpose_zero_is_noop() {
    Command::cargo_bin("chordpro")
        .unwrap()
        .args([&fixture("simple.cho"), "--transpose", "0"])
        .assert()
        .success()
        .stdout(predicate::str::contains("G     C"))
        .stdout(predicate::str::contains("Hello world"));
}

#[test]
fn test_no_args_shows_error() {
    Command::cargo_bin("chordpro")
        .unwrap()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}
