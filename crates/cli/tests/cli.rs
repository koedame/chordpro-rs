//! Integration tests for the `chordpro` CLI binary.

use assert_cmd::Command;
use predicates::prelude::*;
use std::io::Write;
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

// --- HTML format ---

#[test]
fn test_format_html_stdout() {
    Command::cargo_bin("chordpro")
        .unwrap()
        .args(["--format", "html", &fixture("simple.cho")])
        .assert()
        .success()
        .stdout(predicate::str::contains("<!DOCTYPE html>"))
        .stdout(predicate::str::contains("<h1>Simple Song</h1>"))
        .stdout(predicate::str::contains("chord-block"));
}

#[test]
fn test_format_html_output_file() {
    let output_file = NamedTempFile::new().unwrap();
    let output_path = output_file.path().to_string_lossy().to_string();

    Command::cargo_bin("chordpro")
        .unwrap()
        .args([
            "--format",
            "html",
            "-o",
            &output_path,
            &fixture("simple.cho"),
        ])
        .assert()
        .success();

    let content = std::fs::read_to_string(&output_path).unwrap();
    assert!(content.contains("<!DOCTYPE html>"));
}

// --- PDF format ---

#[test]
fn test_format_pdf_output_file() {
    let output_file = NamedTempFile::new().unwrap();
    let output_path = output_file.path().to_string_lossy().to_string();

    Command::cargo_bin("chordpro")
        .unwrap()
        .args([
            "--format",
            "pdf",
            "-o",
            &output_path,
            &fixture("simple.cho"),
        ])
        .assert()
        .success();

    let content = std::fs::read(&output_path).unwrap();
    assert!(content.starts_with(b"%PDF"));
}

#[test]
fn test_format_pdf_with_transpose() {
    let output_file = NamedTempFile::new().unwrap();
    let output_path = output_file.path().to_string_lossy().to_string();

    Command::cargo_bin("chordpro")
        .unwrap()
        .args([
            "--format",
            "pdf",
            "-t",
            "2",
            "-o",
            &output_path,
            &fixture("simple.cho"),
        ])
        .assert()
        .success();

    let content = std::fs::read(&output_path).unwrap();
    assert!(content.starts_with(b"%PDF"));
}

// --- --config, --define, --no-default-configs ---

#[test]
fn test_config_preset() {
    Command::cargo_bin("chordpro")
        .unwrap()
        .args(["--config", "guitar", &fixture("simple.cho")])
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello world"));
}

#[test]
fn test_config_file() {
    let mut config_file = NamedTempFile::new().unwrap();
    write!(config_file, r#"{{ "settings": {{ "transpose": 2 }} }}"#).unwrap();
    config_file.flush().unwrap();

    // Config sets transpose=2, so G→A and C→D
    Command::cargo_bin("chordpro")
        .unwrap()
        .args([
            "--config",
            config_file.path().to_str().unwrap(),
            &fixture("simple.cho"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("A     D"))
        .stdout(predicate::str::contains("Hello world"));
}

#[test]
fn test_config_nonexistent_file() {
    Command::cargo_bin("chordpro")
        .unwrap()
        .args([
            "--config",
            "/nonexistent/chordpro-test-config.json",
            &fixture("simple.cho"),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error:"));
}

#[test]
fn test_define_valid() {
    // --define settings.transpose=3 should transpose G→A# and C→D#
    Command::cargo_bin("chordpro")
        .unwrap()
        .args(["--define", "settings.transpose=3", &fixture("simple.cho")])
        .assert()
        .success()
        .stdout(predicate::str::contains("A#"))
        .stdout(predicate::str::contains("Hello world"));
}

#[test]
fn test_define_invalid_syntax() {
    Command::cargo_bin("chordpro")
        .unwrap()
        .args(["--define", "noequalssign", &fixture("simple.cho")])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error:"))
        .stderr(predicate::str::contains("key=value"));
}

#[test]
fn test_define_empty_key_rejected() {
    Command::cargo_bin("chordpro")
        .unwrap()
        .args(["--define", "=value", &fixture("simple.cho")])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error:"))
        .stderr(predicate::str::contains("key must not be empty"));
}

#[test]
fn test_define_whitespace_key_rejected() {
    Command::cargo_bin("chordpro")
        .unwrap()
        .args(["--define", "  =value", &fixture("simple.cho")])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error:"))
        .stderr(predicate::str::contains("key must not be empty"));
}

#[test]
fn test_config_transpose_combined_with_cli() {
    let mut config_file = NamedTempFile::new().unwrap();
    // Config sets transpose=2
    write!(config_file, r#"{{ "settings": {{ "transpose": 2 }} }}"#).unwrap();
    config_file.flush().unwrap();

    // CLI adds --transpose 1, total = 3 semitones: G→A# and C→D#
    Command::cargo_bin("chordpro")
        .unwrap()
        .args([
            "--config",
            config_file.path().to_str().unwrap(),
            "--transpose",
            "1",
            &fixture("simple.cho"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("A#"))
        .stdout(predicate::str::contains("Hello world"));
}

#[test]
fn test_config_transpose_out_of_range_positive() {
    let mut config_file = NamedTempFile::new().unwrap();
    write!(config_file, r#"{{ "settings": {{ "transpose": 300 }} }}"#).unwrap();
    config_file.flush().unwrap();

    Command::cargo_bin("chordpro")
        .unwrap()
        .args([
            "--config",
            config_file.path().to_str().unwrap(),
            &fixture("simple.cho"),
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "warning: settings.transpose value 300 is out of i8 range, clamped to 127",
        ));
}

#[test]
fn test_config_transpose_out_of_range_negative() {
    let mut config_file = NamedTempFile::new().unwrap();
    write!(config_file, r#"{{ "settings": {{ "transpose": -200 }} }}"#).unwrap();
    config_file.flush().unwrap();

    Command::cargo_bin("chordpro")
        .unwrap()
        .args([
            "--config",
            config_file.path().to_str().unwrap(),
            &fixture("simple.cho"),
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "warning: settings.transpose value -200 is out of i8 range, clamped to -128",
        ));
}

#[test]
fn test_no_default_configs() {
    Command::cargo_bin("chordpro")
        .unwrap()
        .args(["--no-default-configs", &fixture("simple.cho")])
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello world"));
}

// --- Song-level config overrides ---

#[test]
fn test_song_config_override_transpose() {
    // {+config.settings.transpose: 2} should transpose G→A and C→D
    Command::cargo_bin("chordpro")
        .unwrap()
        .arg(fixture("config-override.cho"))
        .assert()
        .success()
        .stdout(predicate::str::contains("A     D"))
        .stdout(predicate::str::contains("Hello world"));
}

#[test]
fn test_song_config_override_combined_with_cli_transpose() {
    // Song has {+config.settings.transpose: 2}, CLI adds --transpose 1
    // Total = 3 semitones: G→A# and C→D#
    Command::cargo_bin("chordpro")
        .unwrap()
        .args([&fixture("config-override.cho"), "--transpose", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("A#"))
        .stdout(predicate::str::contains("Hello world"));
}
