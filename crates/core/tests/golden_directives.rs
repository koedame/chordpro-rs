//! Golden tests for standard directive parsing.
//!
//! These tests parse `.cho` fixture files and verify that the parser correctly
//! classifies directives, populates metadata, and handles short aliases.

use chordpro_core::ast::{CommentStyle, DirectiveKind, Line};
use chordpro_core::parser::parse;

/// Reads a fixture file relative to the fixtures directory.
fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read fixture {path}: {e}"))
}

#[test]
fn standard_directives_golden_test() {
    let input = fixture("standard_directives.cho");
    let song = parse(&input).expect("parse failed");

    // -- Metadata -----------------------------------------------------------
    assert_eq!(song.metadata.title.as_deref(), Some("Amazing Grace"));
    assert_eq!(song.metadata.subtitles, vec!["A Hymn"]);
    assert_eq!(song.metadata.artists, vec!["John Newton"]);
    assert_eq!(song.metadata.composers, vec!["Unknown"]);
    assert_eq!(song.metadata.lyricists, vec!["John Newton"]);
    assert_eq!(song.metadata.album.as_deref(), Some("Hymns"));
    assert_eq!(song.metadata.year.as_deref(), Some("1779"));
    assert_eq!(song.metadata.key.as_deref(), Some("G"));
    assert_eq!(song.metadata.tempo.as_deref(), Some("80"));
    assert_eq!(song.metadata.time.as_deref(), Some("3/4"));
    assert_eq!(song.metadata.capo.as_deref(), Some("2"));

    // -- Directive classification -------------------------------------------
    // Lines 0-10: metadata directives
    for i in 0..=10 {
        assert!(
            matches!(song.lines[i], Line::Directive(_)),
            "line {i} should be a directive, got: {:?}",
            song.lines[i]
        );
    }

    // Check the directive kinds for first few
    if let Line::Directive(ref d) = song.lines[0] {
        assert_eq!(d.kind, DirectiveKind::Title);
        assert_eq!(d.name, "title");
    }
    if let Line::Directive(ref d) = song.lines[1] {
        assert_eq!(d.kind, DirectiveKind::Subtitle);
    }
    if let Line::Directive(ref d) = song.lines[2] {
        assert_eq!(d.kind, DirectiveKind::Artist);
    }

    // Lines 11-13: comment directives
    assert_eq!(
        song.lines[11],
        Line::Comment(CommentStyle::Normal, "Verse 1".to_string())
    );
    assert_eq!(
        song.lines[12],
        Line::Comment(CommentStyle::Italic, "Play softly".to_string())
    );
    assert_eq!(
        song.lines[13],
        Line::Comment(CommentStyle::Boxed, "Key change ahead".to_string())
    );

    // Line 14: empty
    assert_eq!(song.lines[14], Line::Empty);

    // Line 15: start_of_verse
    if let Line::Directive(ref d) = song.lines[15] {
        assert_eq!(d.kind, DirectiveKind::StartOfVerse);
        assert_eq!(d.name, "start_of_verse");
        assert!(d.is_section_start());
        assert_eq!(d.section_name(), Some("verse"));
    } else {
        panic!("expected start_of_verse directive");
    }

    // Line 18: end_of_verse
    if let Line::Directive(ref d) = song.lines[18] {
        assert_eq!(d.kind, DirectiveKind::EndOfVerse);
        assert!(d.is_section_end());
    } else {
        panic!("expected end_of_verse directive");
    }

    // Line 20: start_of_chorus
    if let Line::Directive(ref d) = song.lines[20] {
        assert_eq!(d.kind, DirectiveKind::StartOfChorus);
    } else {
        panic!("expected start_of_chorus directive");
    }

    // Line 22: end_of_chorus
    if let Line::Directive(ref d) = song.lines[22] {
        assert_eq!(d.kind, DirectiveKind::EndOfChorus);
    } else {
        panic!("expected end_of_chorus directive");
    }
}

#[test]
fn short_aliases_golden_test() {
    let input = fixture("short_aliases.cho");
    let song = parse(&input).expect("parse failed");

    // Metadata via short aliases
    assert_eq!(song.metadata.title.as_deref(), Some("Short Title"));
    assert_eq!(song.metadata.subtitles, vec!["Short Subtitle"]);

    // {t: Short Title} -> Directive with kind Title, canonical name "title"
    if let Line::Directive(ref d) = song.lines[0] {
        assert_eq!(d.kind, DirectiveKind::Title);
        assert_eq!(d.name, "title");
        assert_eq!(d.value.as_deref(), Some("Short Title"));
    } else {
        panic!("expected title directive");
    }

    // {st: Short Subtitle}
    if let Line::Directive(ref d) = song.lines[1] {
        assert_eq!(d.kind, DirectiveKind::Subtitle);
        assert_eq!(d.name, "subtitle");
    } else {
        panic!("expected subtitle directive");
    }

    // {c: A comment}
    assert_eq!(
        song.lines[2],
        Line::Comment(CommentStyle::Normal, "A comment".to_string())
    );

    // {ci: Italic comment}
    assert_eq!(
        song.lines[3],
        Line::Comment(CommentStyle::Italic, "Italic comment".to_string())
    );

    // {cb: Boxed comment}
    assert_eq!(
        song.lines[4],
        Line::Comment(CommentStyle::Boxed, "Boxed comment".to_string())
    );

    // {soc} -> start_of_chorus
    if let Line::Directive(ref d) = song.lines[5] {
        assert_eq!(d.kind, DirectiveKind::StartOfChorus);
        assert_eq!(d.name, "start_of_chorus");
    } else {
        panic!("expected start_of_chorus directive");
    }

    // {eoc} -> end_of_chorus
    if let Line::Directive(ref d) = song.lines[7] {
        assert_eq!(d.kind, DirectiveKind::EndOfChorus);
        assert_eq!(d.name, "end_of_chorus");
    } else {
        panic!("expected end_of_chorus directive");
    }

    // {sov} -> start_of_verse
    if let Line::Directive(ref d) = song.lines[8] {
        assert_eq!(d.kind, DirectiveKind::StartOfVerse);
    } else {
        panic!("expected start_of_verse directive");
    }

    // {eov} -> end_of_verse
    if let Line::Directive(ref d) = song.lines[10] {
        assert_eq!(d.kind, DirectiveKind::EndOfVerse);
    } else {
        panic!("expected end_of_verse directive");
    }

    // {sob} -> start_of_bridge
    if let Line::Directive(ref d) = song.lines[11] {
        assert_eq!(d.kind, DirectiveKind::StartOfBridge);
    } else {
        panic!("expected start_of_bridge directive");
    }

    // {eob} -> end_of_bridge
    if let Line::Directive(ref d) = song.lines[13] {
        assert_eq!(d.kind, DirectiveKind::EndOfBridge);
    } else {
        panic!("expected end_of_bridge directive");
    }

    // {sot} -> start_of_tab
    if let Line::Directive(ref d) = song.lines[14] {
        assert_eq!(d.kind, DirectiveKind::StartOfTab);
    } else {
        panic!("expected start_of_tab directive");
    }

    // {eot} -> end_of_tab
    if let Line::Directive(ref d) = song.lines[16] {
        assert_eq!(d.kind, DirectiveKind::EndOfTab);
    } else {
        panic!("expected end_of_tab directive");
    }
}
