//! Integration tests for the MusicXML ↔ ChordPro converter.
//!
//! Tests cover three fixture files plus a round-trip (ChordPro → MusicXML → ChordPro).

use chordsketch_convert_musicxml::{from_musicxml, to_musicxml};
use chordsketch_core::ast::Line;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn fixture(name: &str) -> String {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read fixture {name}: {e}"))
}

// ---------------------------------------------------------------------------
// Fixture: simple.xml
// ---------------------------------------------------------------------------

#[test]
fn simple_import_chords() {
    let xml = fixture("simple.xml");
    let song = from_musicxml(&xml).expect("simple.xml should parse");

    let lyrics: Vec<&Line> = song
        .lines
        .iter()
        .filter(|l| matches!(l, Line::Lyrics(_)))
        .collect();

    // Two measures → two lyric lines
    assert_eq!(lyrics.len(), 2, "expected 2 lyric lines");

    // First line: [C]Hello [Am]world
    if let Line::Lyrics(ll) = lyrics[0] {
        assert_eq!(ll.segments.len(), 2);
        assert_eq!(
            ll.segments[0].chord.as_ref().map(|c| c.name.as_str()),
            Some("C")
        );
        assert!(
            ll.segments[0].text.contains("Hello"),
            "expected 'Hello' in first segment"
        );
        assert_eq!(
            ll.segments[1].chord.as_ref().map(|c| c.name.as_str()),
            Some("Am")
        );
        assert!(
            ll.segments[1].text.contains("world"),
            "expected 'world' in second segment"
        );
    }

    // Second line: [F]Good [G]bye
    if let Line::Lyrics(ll) = lyrics[1] {
        assert_eq!(ll.segments.len(), 2);
        assert_eq!(
            ll.segments[0].chord.as_ref().map(|c| c.name.as_str()),
            Some("F")
        );
        assert_eq!(
            ll.segments[1].chord.as_ref().map(|c| c.name.as_str()),
            Some("G")
        );
    }
}

// ---------------------------------------------------------------------------
// Fixture: metadata.xml
// ---------------------------------------------------------------------------

#[test]
fn metadata_import() {
    let xml = fixture("metadata.xml");
    let song = from_musicxml(&xml).expect("metadata.xml should parse");

    assert_eq!(song.metadata.title.as_deref(), Some("Amazing Grace"));
    assert!(
        song.metadata.artists.iter().any(|a| a.contains("Newton"))
            || song.metadata.lyricists.iter().any(|l| l.contains("Newton")),
        "expected John Newton as composer or lyricist"
    );
    assert_eq!(
        song.metadata.key.as_deref(),
        Some("G"),
        "key should be G major (1 sharp)"
    );
    assert_eq!(song.metadata.tempo.as_deref(), Some("72"));
}

#[test]
fn metadata_import_lyrics_content() {
    let xml = fixture("metadata.xml");
    let song = from_musicxml(&xml).expect("metadata.xml should parse");

    // Should have lyric content including the word "grace"
    let all_text: String = song
        .lines
        .iter()
        .filter_map(|l| {
            if let Line::Lyrics(ll) = l {
                Some(
                    ll.segments
                        .iter()
                        .map(|s| s.text.as_str())
                        .collect::<String>(),
                )
            } else {
                None
            }
        })
        .collect();

    assert!(
        all_text.to_lowercase().contains("grace"),
        "lyrics should contain 'grace'"
    );
}

// ---------------------------------------------------------------------------
// Fixture: sections.xml
// ---------------------------------------------------------------------------

#[test]
fn sections_import_structure() {
    let xml = fixture("sections.xml");
    let song = from_musicxml(&xml).expect("sections.xml should parse");

    assert_eq!(song.metadata.title.as_deref(), Some("Section Demo"));

    // Should have section directives
    let has_section_start = song.lines.iter().any(|l| {
        if let Line::Directive(d) = l {
            use chordsketch_core::ast::DirectiveKind;
            matches!(
                d.kind,
                DirectiveKind::StartOfVerse | DirectiveKind::StartOfChorus
            )
        } else {
            false
        }
    });
    assert!(
        has_section_start,
        "expected at least one section start directive"
    );
}

#[test]
fn sections_import_chorus_label() {
    let xml = fixture("sections.xml");
    let song = from_musicxml(&xml).expect("sections.xml should parse");

    // The Chorus rehearsal mark should produce a start_of_chorus directive
    let has_chorus = song.lines.iter().any(|l| {
        if let Line::Directive(d) = l {
            use chordsketch_core::ast::DirectiveKind;
            d.kind == DirectiveKind::StartOfChorus
        } else {
            false
        }
    });
    assert!(
        has_chorus,
        "expected start_of_chorus directive from 'Chorus' rehearsal mark"
    );
}

// ---------------------------------------------------------------------------
// Round-trip: ChordPro → MusicXML → ChordPro
// ---------------------------------------------------------------------------

#[test]
fn round_trip_preserves_title() {
    let mut song = chordsketch_core::ast::Song::new();
    song.metadata.title = Some("Round Trip Song".to_string());
    song.metadata.artists = vec!["Test Artist".to_string()];
    song.metadata.key = Some("Am".to_string());
    song.metadata.tempo = Some("100".to_string());

    let mut ll = chordsketch_core::ast::LyricsLine::new();
    ll.segments = vec![
        chordsketch_core::ast::LyricsSegment::new(
            Some(chordsketch_core::ast::Chord::new("Am")),
            "Hello ",
        ),
        chordsketch_core::ast::LyricsSegment::new(
            Some(chordsketch_core::ast::Chord::new("Dm")),
            "world ",
        ),
        chordsketch_core::ast::LyricsSegment::new(
            Some(chordsketch_core::ast::Chord::new("E")),
            "yeah",
        ),
    ];
    song.lines.push(Line::Lyrics(ll));

    // Export to MusicXML
    let xml = to_musicxml(&song);

    // Import back
    let reimported = from_musicxml(&xml).expect("round-trip should succeed");

    // Title preserved
    assert_eq!(
        reimported.metadata.title.as_deref(),
        Some("Round Trip Song"),
        "title not preserved in round-trip"
    );

    // Key preserved
    assert_eq!(
        reimported.metadata.key.as_deref(),
        Some("Am"),
        "key not preserved in round-trip"
    );

    // Tempo preserved
    assert_eq!(
        reimported.metadata.tempo.as_deref(),
        Some("100"),
        "tempo not preserved in round-trip"
    );

    // Chords preserved
    let lyrics: Vec<&Line> = reimported
        .lines
        .iter()
        .filter(|l| matches!(l, Line::Lyrics(_)))
        .collect();
    assert!(
        !lyrics.is_empty(),
        "expected at least one lyrics line after round-trip"
    );

    if let Line::Lyrics(ll) = lyrics[0] {
        assert!(
            ll.segments
                .iter()
                .any(|s| s.chord.as_ref().map(|c| c.name.as_str()) == Some("Am")),
            "Am chord should survive round-trip"
        );
        assert!(
            ll.segments
                .iter()
                .any(|s| s.chord.as_ref().map(|c| c.name.as_str()) == Some("Dm")),
            "Dm chord should survive round-trip"
        );
        assert!(
            ll.segments
                .iter()
                .any(|s| s.chord.as_ref().map(|c| c.name.as_str()) == Some("E")),
            "E chord should survive round-trip"
        );
    }
}

#[test]
fn round_trip_preserves_lyrics() {
    let mut song = chordsketch_core::ast::Song::new();
    let mut ll = chordsketch_core::ast::LyricsLine::new();
    ll.segments = vec![
        chordsketch_core::ast::LyricsSegment::new(
            Some(chordsketch_core::ast::Chord::new("C")),
            "Twinkle ",
        ),
        chordsketch_core::ast::LyricsSegment::new(None, "twinkle "),
        chordsketch_core::ast::LyricsSegment::new(
            Some(chordsketch_core::ast::Chord::new("G")),
            "little ",
        ),
        chordsketch_core::ast::LyricsSegment::new(None, "star"),
    ];
    song.lines.push(Line::Lyrics(ll));

    let xml = to_musicxml(&song);
    let reimported = from_musicxml(&xml).expect("lyrics round-trip should succeed");

    let all_text: String = reimported
        .lines
        .iter()
        .filter_map(|l| {
            if let Line::Lyrics(ll) = l {
                Some(
                    ll.segments
                        .iter()
                        .map(|s| s.text.as_str())
                        .collect::<String>(),
                )
            } else {
                None
            }
        })
        .collect();

    assert!(
        all_text.contains("Twinkle"),
        "lyrics 'Twinkle' should survive round-trip"
    );
    assert!(
        all_text.contains("star"),
        "lyrics 'star' should survive round-trip"
    );
}

// ---------------------------------------------------------------------------
// Section end directives
// ---------------------------------------------------------------------------

/// Regression test: imported sections must have explicit `{end_of_*}` directives.
///
/// Previously `map_section_label` returned the end-directive name but it was
/// always discarded with `let _ = section_end`, so the resulting ChordPro had
/// `{start_of_verse}` / `{start_of_chorus}` without any closing directives.
#[test]
fn sections_have_end_directives() {
    let xml = fixture("sections.xml");
    let song = from_musicxml(&xml).expect("sections.xml should parse");

    use chordsketch_core::ast::DirectiveKind;

    let has_verse_end = song.lines.iter().any(|l| {
        matches!(
            l,
            Line::Directive(d) if d.kind == DirectiveKind::EndOfVerse
        )
    });
    let has_chorus_end = song.lines.iter().any(|l| {
        matches!(
            l,
            Line::Directive(d) if d.kind == DirectiveKind::EndOfChorus
        )
    });

    assert!(
        has_verse_end,
        "expected end_of_verse directive after verse section"
    );
    assert!(
        has_chorus_end,
        "expected end_of_chorus directive after chorus section"
    );
}

/// Sections must be ordered correctly: start → content → end.
#[test]
fn section_end_follows_content() {
    let xml = fixture("sections.xml");
    let song = from_musicxml(&xml).expect("sections.xml should parse");

    use chordsketch_core::ast::DirectiveKind;

    // Collect line positions for start_of_verse, lyrics, and end_of_verse.
    let verse_start = song
        .lines
        .iter()
        .position(|l| matches!(l, Line::Directive(d) if d.kind == DirectiveKind::StartOfVerse));
    let verse_end = song
        .lines
        .iter()
        .position(|l| matches!(l, Line::Directive(d) if d.kind == DirectiveKind::EndOfVerse));
    let first_lyrics = song.lines.iter().position(|l| matches!(l, Line::Lyrics(_)));

    let (start, end, lyrics) = (
        verse_start.expect("start_of_verse not found"),
        verse_end.expect("end_of_verse not found"),
        first_lyrics.expect("no lyrics found"),
    );
    assert!(
        start < lyrics,
        "start_of_verse must precede lyrics (start={start}, lyrics={lyrics})"
    );
    assert!(
        lyrics < end,
        "lyrics must precede end_of_verse (lyrics={lyrics}, end={end})"
    );
}

// ---------------------------------------------------------------------------
// Error cases
// ---------------------------------------------------------------------------

#[test]
fn wrong_root_element_returns_error() {
    let xml = r#"<?xml version="1.0"?><score-timewise></score-timewise>"#;
    assert!(
        from_musicxml(xml).is_err(),
        "score-timewise should return an error (only score-partwise is supported)"
    );
}

#[test]
fn invalid_xml_returns_error() {
    assert!(from_musicxml("<unclosed").is_err());
}

// ---------------------------------------------------------------------------
// Round-trip: section labels
// ---------------------------------------------------------------------------

/// Regression test: `{start_of_verse}` and `{start_of_chorus}` must survive a
/// ChordPro → MusicXML → ChordPro round-trip in the correct order.
///
/// The medium bug fixed in PR #1460 (rehearsal mark placed on wrong measure) was
/// not caught by round-trip tests at the time.  This test ensures it cannot
/// regress silently.
#[test]
fn round_trip_section_labels_preserved_and_ordered() {
    use chordsketch_core::ast::{Directive, DirectiveKind, LyricsLine, LyricsSegment};

    let mut song = chordsketch_core::ast::Song::new();

    // Build: {start_of_verse} / verse line / {end_of_verse}
    //        {start_of_chorus} / chorus line / {end_of_chorus}
    song.lines
        .push(Line::Directive(Directive::name_only("start_of_verse")));
    let mut verse_line = LyricsLine::new();
    verse_line.segments = vec![LyricsSegment::new(
        Some(chordsketch_core::ast::Chord::new("C")),
        "Verse lyrics",
    )];
    song.lines.push(Line::Lyrics(verse_line));
    song.lines
        .push(Line::Directive(Directive::name_only("end_of_verse")));
    song.lines.push(Line::Empty);
    song.lines
        .push(Line::Directive(Directive::name_only("start_of_chorus")));
    let mut chorus_line = LyricsLine::new();
    chorus_line.segments = vec![LyricsSegment::new(
        Some(chordsketch_core::ast::Chord::new("G")),
        "Chorus lyrics",
    )];
    song.lines.push(Line::Lyrics(chorus_line));
    song.lines
        .push(Line::Directive(Directive::name_only("end_of_chorus")));

    // Export → import round-trip
    let xml = to_musicxml(&song);
    let reimported = from_musicxml(&xml).expect("section round-trip should succeed");

    // Both section starts must be present after the round-trip.
    let verse_start_pos = reimported
        .lines
        .iter()
        .position(|l| matches!(l, Line::Directive(d) if d.kind == DirectiveKind::StartOfVerse));
    let chorus_start_pos = reimported
        .lines
        .iter()
        .position(|l| matches!(l, Line::Directive(d) if d.kind == DirectiveKind::StartOfChorus));

    let verse_pos = verse_start_pos.expect("StartOfVerse not found after round-trip");
    let chorus_pos = chorus_start_pos.expect("StartOfChorus not found after round-trip");

    assert!(
        verse_pos < chorus_pos,
        "StartOfVerse (pos {verse_pos}) must precede StartOfChorus (pos {chorus_pos})"
    );
}
