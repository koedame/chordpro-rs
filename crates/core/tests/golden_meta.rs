//! Golden tests for the `{meta: key value}` directive.
//!
//! These tests verify that the parser correctly handles the generic `{meta}`
//! directive, routing known metadata keys to their respective fields and
//! storing unknown keys in the custom metadata vector.

use chordpro_core::ast::{DirectiveKind, Line};
use chordpro_core::parser::parse;

/// Reads a fixture file relative to the fixtures directory.
fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read fixture {path}: {e}"))
}

#[test]
fn meta_directive_known_keys_populate_metadata() {
    let input = fixture("meta-directive/input.cho");
    let song = parse(&input).expect("parse failed");

    // Title comes from {title: Test Song}, not from meta
    assert_eq!(song.metadata.title.as_deref(), Some("Test Song"));

    // These come from {meta: ...} directives
    assert_eq!(song.metadata.artists, vec!["Someone Famous"]);
    assert_eq!(song.metadata.key.as_deref(), Some("Am"));
    assert_eq!(song.metadata.composers, vec!["J.S. Bach"]);
    assert_eq!(song.metadata.lyricists, vec!["William Blake"]);
    assert_eq!(song.metadata.album.as_deref(), Some("Greatest Hits"));
    assert_eq!(song.metadata.year.as_deref(), Some("2024"));
    assert_eq!(song.metadata.tempo.as_deref(), Some("120"));
    assert_eq!(song.metadata.time.as_deref(), Some("4/4"));
    assert_eq!(song.metadata.capo.as_deref(), Some("3"));
}

#[test]
fn meta_directive_custom_keys_go_to_custom() {
    let input = fixture("meta-directive/input.cho");
    let song = parse(&input).expect("parse failed");

    assert_eq!(
        song.metadata.custom,
        vec![
            ("custom_field".to_string(), "some custom value".to_string()),
            ("mood".to_string(), "melancholy".to_string()),
        ]
    );
}

#[test]
fn meta_directive_kind_is_meta() {
    let input = fixture("meta-directive/input.cho");
    let song = parse(&input).expect("parse failed");

    // Line 1 (index 1) is {meta: artist Someone Famous}
    if let Line::Directive(ref d) = song.lines[1] {
        assert_eq!(d.name, "meta");
        assert_eq!(d.kind, DirectiveKind::Meta("artist".to_string()));
        assert_eq!(d.value.as_deref(), Some("Someone Famous"));
    } else {
        panic!(
            "expected meta directive at index 1, got: {:?}",
            song.lines[1]
        );
    }

    // Line 2 (index 2) is {meta: key Am}
    if let Line::Directive(ref d) = song.lines[2] {
        assert_eq!(d.name, "meta");
        assert_eq!(d.kind, DirectiveKind::Meta("key".to_string()));
        assert_eq!(d.value.as_deref(), Some("Am"));
    } else {
        panic!(
            "expected meta directive at index 2, got: {:?}",
            song.lines[2]
        );
    }
}

#[test]
fn meta_directive_is_metadata() {
    let kind = DirectiveKind::Meta("artist".to_string());
    assert!(kind.is_metadata());

    let kind = DirectiveKind::Meta("custom_field".to_string());
    assert!(kind.is_metadata());
}

#[test]
fn meta_directive_canonical_name() {
    let kind = DirectiveKind::Meta("artist".to_string());
    assert_eq!(kind.canonical_name(), "meta");
}

#[test]
fn meta_directive_key_only_no_value() {
    // {meta: solo_key} — key with no value
    let song = parse("{meta: solo_key}").expect("parse failed");

    if let Line::Directive(ref d) = song.lines[0] {
        assert_eq!(d.name, "meta");
        assert_eq!(d.kind, DirectiveKind::Meta("solo_key".to_string()));
        assert!(d.value.is_none());
    } else {
        panic!("expected meta directive");
    }

    // Since there's no value, custom metadata should not be populated
    assert!(song.metadata.custom.is_empty());
}

#[test]
fn meta_directive_without_value_is_unknown() {
    // {meta} with no value at all — treated as unknown
    let song = parse("{meta}").expect("parse failed");

    if let Line::Directive(ref d) = song.lines[0] {
        assert_eq!(d.kind, DirectiveKind::Unknown("meta".to_string()));
    } else {
        panic!("expected directive");
    }
}

#[test]
fn meta_directive_overrides_existing_metadata() {
    // {key: G} followed by {meta: key Am} — meta should override
    let song = parse("{key: G}\n{meta: key Am}").expect("parse failed");
    assert_eq!(song.metadata.key.as_deref(), Some("Am"));
}

#[test]
fn meta_directive_appends_to_vec_fields() {
    // Multiple artists via meta
    let song = parse("{meta: artist Alice}\n{meta: artist Bob}").expect("parse failed");
    assert_eq!(song.metadata.artists, vec!["Alice", "Bob"]);
}

#[test]
fn meta_directive_title_via_meta() {
    // {meta: title My Song} — sets title through meta
    let song = parse("{meta: title My Song}").expect("parse failed");
    assert_eq!(song.metadata.title.as_deref(), Some("My Song"));

    if let Line::Directive(ref d) = song.lines[0] {
        assert_eq!(d.kind, DirectiveKind::Meta("title".to_string()));
        assert_eq!(d.value.as_deref(), Some("My Song"));
    } else {
        panic!("expected meta directive");
    }
}

#[test]
fn meta_directive_subtitle_via_meta() {
    // {meta: subtitle Another Name} — sets subtitle through meta
    let song = parse("{meta: subtitle Another Name}").expect("parse failed");
    assert_eq!(song.metadata.subtitles, vec!["Another Name"]);
}
