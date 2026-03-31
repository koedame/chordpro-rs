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

    // {sog} -> start_of_grid
    if let Line::Directive(ref d) = song.lines[17] {
        assert_eq!(d.kind, DirectiveKind::StartOfGrid);
        assert_eq!(d.name, "start_of_grid");
    } else {
        panic!("expected start_of_grid directive");
    }

    // {eog} -> end_of_grid
    if let Line::Directive(ref d) = song.lines[19] {
        assert_eq!(d.kind, DirectiveKind::EndOfGrid);
        assert_eq!(d.name, "end_of_grid");
    } else {
        panic!("expected end_of_grid directive");
    }
}

#[test]
fn extended_metadata_directives_golden_test() {
    let input = fixture("metadata-extended/input.cho");
    let song = parse(&input).expect("parse failed");

    // -- Metadata -----------------------------------------------------------
    assert_eq!(song.metadata.title.as_deref(), Some("Test Song"));
    assert_eq!(song.metadata.sort_title.as_deref(), Some("Song, Test"));
    assert_eq!(song.metadata.artists, vec!["Jane Doe"]);
    assert_eq!(song.metadata.sort_artist.as_deref(), Some("Doe, Jane"));
    assert_eq!(song.metadata.arrangers, vec!["John Smith", "Bob Jones"]);
    assert_eq!(song.metadata.copyright.as_deref(), Some("2024 Jane Doe"));
    assert_eq!(song.metadata.duration.as_deref(), Some("3:45"));
    assert_eq!(song.metadata.tags, vec!["folk", "acoustic"]);

    // -- Directive classification -------------------------------------------
    // Line 0: title
    if let Line::Directive(ref d) = song.lines[0] {
        assert_eq!(d.kind, DirectiveKind::Title);
        assert_eq!(d.name, "title");
    } else {
        panic!("expected title directive");
    }

    // Line 1: sorttitle
    if let Line::Directive(ref d) = song.lines[1] {
        assert_eq!(d.kind, DirectiveKind::SortTitle);
        assert_eq!(d.name, "sorttitle");
        assert_eq!(d.value.as_deref(), Some("Song, Test"));
    } else {
        panic!("expected sorttitle directive");
    }

    // Line 3: sortartist
    if let Line::Directive(ref d) = song.lines[3] {
        assert_eq!(d.kind, DirectiveKind::SortArtist);
        assert_eq!(d.name, "sortartist");
        assert_eq!(d.value.as_deref(), Some("Doe, Jane"));
    } else {
        panic!("expected sortartist directive");
    }

    // Line 4: arranger (first)
    if let Line::Directive(ref d) = song.lines[4] {
        assert_eq!(d.kind, DirectiveKind::Arranger);
        assert_eq!(d.name, "arranger");
        assert_eq!(d.value.as_deref(), Some("John Smith"));
    } else {
        panic!("expected arranger directive");
    }

    // Line 5: arranger (second)
    if let Line::Directive(ref d) = song.lines[5] {
        assert_eq!(d.kind, DirectiveKind::Arranger);
        assert_eq!(d.value.as_deref(), Some("Bob Jones"));
    } else {
        panic!("expected second arranger directive");
    }

    // Line 6: copyright
    if let Line::Directive(ref d) = song.lines[6] {
        assert_eq!(d.kind, DirectiveKind::Copyright);
        assert_eq!(d.name, "copyright");
        assert_eq!(d.value.as_deref(), Some("2024 Jane Doe"));
    } else {
        panic!("expected copyright directive");
    }

    // Line 7: duration
    if let Line::Directive(ref d) = song.lines[7] {
        assert_eq!(d.kind, DirectiveKind::Duration);
        assert_eq!(d.name, "duration");
        assert_eq!(d.value.as_deref(), Some("3:45"));
    } else {
        panic!("expected duration directive");
    }

    // Line 8: tag (first)
    if let Line::Directive(ref d) = song.lines[8] {
        assert_eq!(d.kind, DirectiveKind::Tag);
        assert_eq!(d.name, "tag");
        assert_eq!(d.value.as_deref(), Some("folk"));
    } else {
        panic!("expected tag directive");
    }

    // Line 9: tag (second)
    if let Line::Directive(ref d) = song.lines[9] {
        assert_eq!(d.kind, DirectiveKind::Tag);
        assert_eq!(d.value.as_deref(), Some("acoustic"));
    } else {
        panic!("expected second tag directive");
    }

    // All new metadata directives should be classified as metadata
    assert!(DirectiveKind::SortTitle.is_metadata());
    assert!(DirectiveKind::SortArtist.is_metadata());
    assert!(DirectiveKind::Arranger.is_metadata());
    assert!(DirectiveKind::Copyright.is_metadata());
    assert!(DirectiveKind::Duration.is_metadata());
    assert!(DirectiveKind::Tag.is_metadata());
}

#[test]
fn formatting_directives_golden_test() {
    let input = fixture("formatting-directives/input.cho");
    let song = parse(&input).expect("parse failed");

    // Line 0: title (metadata)
    if let Line::Directive(ref d) = song.lines[0] {
        assert_eq!(d.kind, DirectiveKind::Title);
    } else {
        panic!("expected title directive");
    }

    // Line 1: titlefont
    if let Line::Directive(ref d) = song.lines[1] {
        assert_eq!(d.kind, DirectiveKind::TitleFont);
        assert_eq!(d.name, "titlefont");
        assert_eq!(d.value.as_deref(), Some("Times New Roman"));
        assert!(d.kind.is_font_size_color());
    } else {
        panic!("expected titlefont directive");
    }

    // Line 2: titlesize
    if let Line::Directive(ref d) = song.lines[2] {
        assert_eq!(d.kind, DirectiveKind::TitleSize);
        assert_eq!(d.name, "titlesize");
        assert_eq!(d.value.as_deref(), Some("18"));
    } else {
        panic!("expected titlesize directive");
    }

    // Line 3: titlecolour
    if let Line::Directive(ref d) = song.lines[3] {
        assert_eq!(d.kind, DirectiveKind::TitleColour);
        assert_eq!(d.name, "titlecolour");
        assert_eq!(d.value.as_deref(), Some("#333333"));
    } else {
        panic!("expected titlecolour directive");
    }

    // Line 4: chorusfont
    if let Line::Directive(ref d) = song.lines[4] {
        assert_eq!(d.kind, DirectiveKind::ChorusFont);
        assert_eq!(d.name, "chorusfont");
        assert_eq!(d.value.as_deref(), Some("Helvetica"));
    } else {
        panic!("expected chorusfont directive");
    }

    // Line 5: chorussize
    if let Line::Directive(ref d) = song.lines[5] {
        assert_eq!(d.kind, DirectiveKind::ChorusSize);
        assert_eq!(d.name, "chorussize");
        assert_eq!(d.value.as_deref(), Some("14"));
    } else {
        panic!("expected chorussize directive");
    }

    // Line 6: choruscolor -> canonical choruscolour
    if let Line::Directive(ref d) = song.lines[6] {
        assert_eq!(d.kind, DirectiveKind::ChorusColour);
        assert_eq!(d.name, "choruscolour");
        assert_eq!(d.value.as_deref(), Some("blue"));
    } else {
        panic!("expected choruscolour directive");
    }

    // Line 7: footerfont
    if let Line::Directive(ref d) = song.lines[7] {
        assert_eq!(d.kind, DirectiveKind::FooterFont);
        assert_eq!(d.name, "footerfont");
        assert_eq!(d.value.as_deref(), Some("Arial"));
    } else {
        panic!("expected footerfont directive");
    }

    // Line 8: footersize
    if let Line::Directive(ref d) = song.lines[8] {
        assert_eq!(d.kind, DirectiveKind::FooterSize);
        assert_eq!(d.value.as_deref(), Some("10"));
    } else {
        panic!("expected footersize directive");
    }

    // Line 9: footercolour
    if let Line::Directive(ref d) = song.lines[9] {
        assert_eq!(d.kind, DirectiveKind::FooterColour);
        assert_eq!(d.name, "footercolour");
        assert_eq!(d.value.as_deref(), Some("gray"));
    } else {
        panic!("expected footercolour directive");
    }

    // Line 10: headerfont
    if let Line::Directive(ref d) = song.lines[10] {
        assert_eq!(d.kind, DirectiveKind::HeaderFont);
        assert_eq!(d.name, "headerfont");
    } else {
        panic!("expected headerfont directive");
    }

    // Line 11: headersize
    if let Line::Directive(ref d) = song.lines[11] {
        assert_eq!(d.kind, DirectiveKind::HeaderSize);
        assert_eq!(d.value.as_deref(), Some("16"));
    } else {
        panic!("expected headersize directive");
    }

    // Line 12: headercolor -> canonical headercolour
    if let Line::Directive(ref d) = song.lines[12] {
        assert_eq!(d.kind, DirectiveKind::HeaderColour);
        assert_eq!(d.name, "headercolour");
    } else {
        panic!("expected headercolour directive");
    }

    // Line 13: labelfont
    if let Line::Directive(ref d) = song.lines[13] {
        assert_eq!(d.kind, DirectiveKind::LabelFont);
        assert_eq!(d.name, "labelfont");
    } else {
        panic!("expected labelfont directive");
    }

    // Line 14: labelsize
    if let Line::Directive(ref d) = song.lines[14] {
        assert_eq!(d.kind, DirectiveKind::LabelSize);
    } else {
        panic!("expected labelsize directive");
    }

    // Line 15: labelcolour
    if let Line::Directive(ref d) = song.lines[15] {
        assert_eq!(d.kind, DirectiveKind::LabelColour);
        assert_eq!(d.name, "labelcolour");
    } else {
        panic!("expected labelcolour directive");
    }

    // Line 16: gridfont
    if let Line::Directive(ref d) = song.lines[16] {
        assert_eq!(d.kind, DirectiveKind::GridFont);
        assert_eq!(d.name, "gridfont");
    } else {
        panic!("expected gridfont directive");
    }

    // Line 17: gridsize
    if let Line::Directive(ref d) = song.lines[17] {
        assert_eq!(d.kind, DirectiveKind::GridSize);
    } else {
        panic!("expected gridsize directive");
    }

    // Line 18: gridcolour
    if let Line::Directive(ref d) = song.lines[18] {
        assert_eq!(d.kind, DirectiveKind::GridColour);
        assert_eq!(d.name, "gridcolour");
    } else {
        panic!("expected gridcolour directive");
    }

    // Line 19: tocfont
    if let Line::Directive(ref d) = song.lines[19] {
        assert_eq!(d.kind, DirectiveKind::TocFont);
        assert_eq!(d.name, "tocfont");
    } else {
        panic!("expected tocfont directive");
    }

    // Line 20: tocsize
    if let Line::Directive(ref d) = song.lines[20] {
        assert_eq!(d.kind, DirectiveKind::TocSize);
    } else {
        panic!("expected tocsize directive");
    }

    // Line 21: toccolor -> canonical toccolour
    if let Line::Directive(ref d) = song.lines[21] {
        assert_eq!(d.kind, DirectiveKind::TocColour);
        assert_eq!(d.name, "toccolour");
        assert_eq!(d.value.as_deref(), Some("navy"));
    } else {
        panic!("expected toccolour directive");
    }

    // Verify none of these are metadata
    for i in 1..=21 {
        if let Line::Directive(ref d) = song.lines[i] {
            assert!(
                d.kind.is_font_size_color(),
                "line {i}: {:?} should be font_size_color",
                d.kind
            );
            assert!(
                !d.kind.is_metadata(),
                "line {i}: {:?} should not be metadata",
                d.kind
            );
        }
    }
}

#[test]
fn page_control_directives_golden_test() {
    let input = fixture("page-control-directives/input.cho");
    let song = parse(&input).expect("parse failed");

    // Collect all page control directives from the parsed output.
    let page_control_directives: Vec<_> = song
        .lines
        .iter()
        .filter_map(|line| {
            if let Line::Directive(d) = line {
                if d.kind.is_page_control() {
                    return Some(d);
                }
            }
            None
        })
        .collect();

    // The fixture should contain at least one of each page control directive.
    let kinds: Vec<_> = page_control_directives.iter().map(|d| &d.kind).collect();
    assert!(
        kinds.contains(&&DirectiveKind::NewPage),
        "expected NewPage directive"
    );
    assert!(
        kinds.contains(&&DirectiveKind::NewPhysicalPage),
        "expected NewPhysicalPage directive"
    );
    assert!(
        kinds.contains(&&DirectiveKind::ColumnBreak),
        "expected ColumnBreak directive"
    );
    assert!(
        kinds.contains(&&DirectiveKind::Columns),
        "expected Columns directive"
    );

    // Verify classification: all page control directives are NOT metadata.
    for d in &page_control_directives {
        assert!(
            d.kind.is_page_control(),
            "{:?} should be page_control",
            d.kind
        );
        assert!(!d.kind.is_metadata(), "{:?} should not be metadata", d.kind);
        assert!(
            !d.kind.is_font_size_color(),
            "{:?} should not be font_size_color",
            d.kind
        );
    }

    // Verify canonical names for short aliases.
    for d in &page_control_directives {
        match d.kind {
            DirectiveKind::NewPage => assert_eq!(d.name, "new_page"),
            DirectiveKind::NewPhysicalPage => assert_eq!(d.name, "new_physical_page"),
            DirectiveKind::ColumnBreak => assert_eq!(d.name, "column_break"),
            DirectiveKind::Columns => assert_eq!(d.name, "columns"),
            _ => {}
        }
    }
}
