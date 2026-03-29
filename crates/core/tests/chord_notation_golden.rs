//! Golden test for chord notation parsing.
//!
//! Verifies that the parser correctly extracts structured chord details from
//! chord annotations in a `.cho` input file.

use chordpro_core::ast::Line;
use chordpro_core::chord::{Accidental, ChordQuality, Note};
use chordpro_core::parse;

/// Reads the fixture file and parses it.
fn parse_fixture() -> chordpro_core::ast::Song {
    let input = include_str!("fixtures/chord_notation.cho");
    parse(input).expect("fixture should parse without errors")
}

#[test]
fn chord_notation_golden_test() {
    let song = parse_fixture();

    // Line 0: {title: Chord Notation Test} — directive
    assert!(matches!(song.lines[0], Line::Directive(_)));

    // Line 1: [C]Basic [Am]minor [G]major
    if let Line::Lyrics(ref lyrics) = song.lines[1] {
        assert_eq!(lyrics.segments.len(), 3);

        let c = lyrics.segments[0].chord.as_ref().unwrap();
        assert_eq!(c.name, "C");
        let d = c.detail.as_ref().unwrap();
        assert_eq!(d.root, Note::C);
        assert_eq!(d.quality, ChordQuality::Major);
        assert_eq!(d.root_accidental, None);

        let am = lyrics.segments[1].chord.as_ref().unwrap();
        assert_eq!(am.name, "Am");
        let d = am.detail.as_ref().unwrap();
        assert_eq!(d.root, Note::A);
        assert_eq!(d.quality, ChordQuality::Minor);

        let g = lyrics.segments[2].chord.as_ref().unwrap();
        assert_eq!(g.name, "G");
        let d = g.detail.as_ref().unwrap();
        assert_eq!(d.root, Note::G);
        assert_eq!(d.quality, ChordQuality::Major);
    } else {
        panic!("Expected Line::Lyrics for line 1");
    }

    // Line 2: [C#]Sharp [Db]flat [F#m]sharp minor
    if let Line::Lyrics(ref lyrics) = song.lines[2] {
        let cs = lyrics.segments[0].chord.as_ref().unwrap();
        let d = cs.detail.as_ref().unwrap();
        assert_eq!(d.root, Note::C);
        assert_eq!(d.root_accidental, Some(Accidental::Sharp));

        let db = lyrics.segments[1].chord.as_ref().unwrap();
        let d = db.detail.as_ref().unwrap();
        assert_eq!(d.root, Note::D);
        assert_eq!(d.root_accidental, Some(Accidental::Flat));

        let fsm = lyrics.segments[2].chord.as_ref().unwrap();
        let d = fsm.detail.as_ref().unwrap();
        assert_eq!(d.root, Note::F);
        assert_eq!(d.root_accidental, Some(Accidental::Sharp));
        assert_eq!(d.quality, ChordQuality::Minor);
    } else {
        panic!("Expected Line::Lyrics for line 2");
    }

    // Line 3: [G/B]Slash [C/E]chord [Am7/G]extended slash
    if let Line::Lyrics(ref lyrics) = song.lines[3] {
        let gb = lyrics.segments[0].chord.as_ref().unwrap();
        let d = gb.detail.as_ref().unwrap();
        assert_eq!(d.root, Note::G);
        assert_eq!(d.bass_note, Some((Note::B, None)));

        let ce = lyrics.segments[1].chord.as_ref().unwrap();
        let d = ce.detail.as_ref().unwrap();
        assert_eq!(d.root, Note::C);
        assert_eq!(d.bass_note, Some((Note::E, None)));

        let am7g = lyrics.segments[2].chord.as_ref().unwrap();
        let d = am7g.detail.as_ref().unwrap();
        assert_eq!(d.root, Note::A);
        assert_eq!(d.quality, ChordQuality::Minor);
        assert_eq!(d.extension.as_deref(), Some("7"));
        assert_eq!(d.bass_note, Some((Note::G, None)));
    } else {
        panic!("Expected Line::Lyrics for line 3");
    }

    // Line 4: [Cmaj7]Major seventh [Am7]minor seventh [G9]ninth
    if let Line::Lyrics(ref lyrics) = song.lines[4] {
        let cmaj7 = lyrics.segments[0].chord.as_ref().unwrap();
        let d = cmaj7.detail.as_ref().unwrap();
        assert_eq!(d.root, Note::C);
        assert_eq!(d.quality, ChordQuality::Major);
        assert_eq!(d.extension.as_deref(), Some("maj7"));

        let am7 = lyrics.segments[1].chord.as_ref().unwrap();
        let d = am7.detail.as_ref().unwrap();
        assert_eq!(d.root, Note::A);
        assert_eq!(d.quality, ChordQuality::Minor);
        assert_eq!(d.extension.as_deref(), Some("7"));

        let g9 = lyrics.segments[2].chord.as_ref().unwrap();
        let d = g9.detail.as_ref().unwrap();
        assert_eq!(d.root, Note::G);
        assert_eq!(d.extension.as_deref(), Some("9"));
    } else {
        panic!("Expected Line::Lyrics for line 4");
    }

    // Line 5: [Dsus4]Suspended [Asus2]sus two [Cadd9]add nine
    if let Line::Lyrics(ref lyrics) = song.lines[5] {
        let dsus4 = lyrics.segments[0].chord.as_ref().unwrap();
        let d = dsus4.detail.as_ref().unwrap();
        assert_eq!(d.root, Note::D);
        assert_eq!(d.extension.as_deref(), Some("sus4"));

        let asus2 = lyrics.segments[1].chord.as_ref().unwrap();
        let d = asus2.detail.as_ref().unwrap();
        assert_eq!(d.root, Note::A);
        assert_eq!(d.extension.as_deref(), Some("sus2"));

        let cadd9 = lyrics.segments[2].chord.as_ref().unwrap();
        let d = cadd9.detail.as_ref().unwrap();
        assert_eq!(d.root, Note::C);
        assert_eq!(d.extension.as_deref(), Some("add9"));
    } else {
        panic!("Expected Line::Lyrics for line 5");
    }

    // Line 6: [Bdim]Diminished [Faug]augmented [C+]plus
    if let Line::Lyrics(ref lyrics) = song.lines[6] {
        let bdim = lyrics.segments[0].chord.as_ref().unwrap();
        let d = bdim.detail.as_ref().unwrap();
        assert_eq!(d.root, Note::B);
        assert_eq!(d.quality, ChordQuality::Diminished);

        let faug = lyrics.segments[1].chord.as_ref().unwrap();
        let d = faug.detail.as_ref().unwrap();
        assert_eq!(d.root, Note::F);
        assert_eq!(d.quality, ChordQuality::Augmented);

        let cplus = lyrics.segments[2].chord.as_ref().unwrap();
        let d = cplus.detail.as_ref().unwrap();
        assert_eq!(d.root, Note::C);
        assert_eq!(d.quality, ChordQuality::Augmented);
    } else {
        panic!("Expected Line::Lyrics for line 6");
    }

    // Line 7: [Bbm]Flat minor [Ebdim]flat dim
    if let Line::Lyrics(ref lyrics) = song.lines[7] {
        let bbm = lyrics.segments[0].chord.as_ref().unwrap();
        let d = bbm.detail.as_ref().unwrap();
        assert_eq!(d.root, Note::B);
        assert_eq!(d.root_accidental, Some(Accidental::Flat));
        assert_eq!(d.quality, ChordQuality::Minor);

        let ebdim = lyrics.segments[1].chord.as_ref().unwrap();
        let d = ebdim.detail.as_ref().unwrap();
        assert_eq!(d.root, Note::E);
        assert_eq!(d.root_accidental, Some(Accidental::Flat));
        assert_eq!(d.quality, ChordQuality::Diminished);
    } else {
        panic!("Expected Line::Lyrics for line 7");
    }
}
