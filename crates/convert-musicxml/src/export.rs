//! ChordPro → MusicXML exporter.
//!
//! Converts a [`Song`] AST into a MusicXML 4.0 `<score-partwise>` document.
//!
//! # What is exported
//!
//! - Song metadata: title, composer/artist, key, tempo, capo
//! - Chord symbols: emitted as `<harmony>` elements using root step/alter
//!   and the MusicXML kind that best matches the ChordPro chord
//! - Lyrics: emitted as `<note>` elements with `<lyric>` children; one note
//!   per lyric segment
//! - Section structure: `{start_of_verse}` / `{start_of_chorus}` /
//!   `{start_of_bridge}` are emitted as `<rehearsal>` direction elements
//!
//! # Note durations
//!
//! Since ChordPro does not carry rhythmic information, all notes are exported
//! as whole notes. Applications that require real note durations must add them
//! after import.

use chordsketch_core::{
    ast::{DirectiveKind, Line, Song},
    chord::parse_chord,
};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Converts a [`Song`] AST into a MusicXML 4.0 string.
///
/// The output is a well-formed XML document that can be opened by any
/// MusicXML-compatible application (e.g., MuseScore, Finale, Sibelius).
#[must_use]
pub fn to_musicxml(song: &Song) -> String {
    let mut out = String::new();
    out.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    out.push('\n');
    out.push_str(
        r#"<!DOCTYPE score-partwise PUBLIC "-//Recordare//DTD MusicXML 4.0 Partwise//EN"
  "http://www.musicxml.org/dtds/partwise.dtd">"#,
    );
    out.push('\n');
    out.push_str(r#"<score-partwise version="4.0">"#);
    out.push('\n');

    // Work
    if let Some(ref title) = song.metadata.title {
        out.push_str("  <work>\n");
        out.push_str("    <work-title>");
        xml_text(title, &mut out);
        out.push_str("</work-title>\n");
        out.push_str("  </work>\n");
    }

    // Identification
    let has_ident = !song.metadata.artists.is_empty() || !song.metadata.lyricists.is_empty();
    if has_ident {
        out.push_str("  <identification>\n");
        for artist in &song.metadata.artists {
            out.push_str(r#"    <creator type="composer">"#);
            xml_text(artist, &mut out);
            out.push_str("</creator>\n");
        }
        for lyricist in &song.metadata.lyricists {
            out.push_str(r#"    <creator type="lyricist">"#);
            xml_text(lyricist, &mut out);
            out.push_str("</creator>\n");
        }
        out.push_str("  </identification>\n");
    }

    // Part list
    out.push_str("  <part-list>\n");
    out.push_str(r#"    <score-part id="P1"><part-name>Voice</part-name></score-part>"#);
    out.push('\n');
    out.push_str("  </part-list>\n");

    // Part
    out.push_str(r#"  <part id="P1">"#);
    out.push('\n');

    let measures = build_measures(song);
    for (i, measure) in measures.iter().enumerate() {
        write_measure(measure, i + 1, &mut out);
    }

    out.push_str("  </part>\n");
    out.push_str("</score-partwise>\n");
    out
}

// ---------------------------------------------------------------------------
// Measure building
// ---------------------------------------------------------------------------

/// A simplified measure for export purposes.
#[derive(Default)]
struct Measure {
    /// Key signature (fifths, mode) — only emitted in the first measure.
    key: Option<(i32, &'static str)>,
    /// Tempo in BPM — only emitted in the first measure.
    tempo: Option<String>,
    /// Section label (becomes a rehearsal mark).
    section_label: Option<String>,
    /// Sequence of (chord_name, lyric_text) pairs.
    notes: Vec<(Option<String>, String)>,
}

/// Convert a [`Song`] into a flat list of measures.
fn build_measures(song: &Song) -> Vec<Measure> {
    let mut measures: Vec<Measure> = Vec::new();
    let mut current = Measure::default();
    let mut first_measure = true;

    // Emit global metadata into the first measure
    if let Some(ref key) = song.metadata.key {
        let (fifths, mode) = key_to_fifths(key);
        current.key = Some((fifths, mode));
    }
    if let Some(ref tempo) = song.metadata.tempo {
        current.tempo = Some(tempo.clone());
    }

    let mut section_name: Option<String> = None;

    for line in &song.lines {
        match line {
            Line::Lyrics(ll) => {
                // Each lyrics line → one measure
                if !current.notes.is_empty() || first_measure {
                    // Flush if there's content already
                    if !current.notes.is_empty() {
                        if let Some(label) = section_name.take() {
                            current.section_label = Some(label);
                        }
                        measures.push(current);
                        current = Measure::default();
                        first_measure = false;
                    }
                }

                // Apply pending section label
                if let Some(label) = section_name.take() {
                    current.section_label = Some(label);
                }

                for seg in &ll.segments {
                    let chord = seg.chord.as_ref().map(|c| c.display_name().to_string());
                    let text = seg.text.clone();
                    current.notes.push((chord, text));
                }
            }

            Line::Directive(dir) => match dir.kind {
                DirectiveKind::StartOfChorus => {
                    section_name = Some(dir.value.clone().unwrap_or_else(|| "Chorus".to_string()));
                }
                DirectiveKind::StartOfVerse => {
                    section_name = Some(dir.value.clone().unwrap_or_else(|| "Verse".to_string()));
                }
                DirectiveKind::StartOfBridge => {
                    section_name = Some(dir.value.clone().unwrap_or_else(|| "Bridge".to_string()));
                }
                DirectiveKind::EndOfChorus
                | DirectiveKind::EndOfVerse
                | DirectiveKind::EndOfBridge => {}
                DirectiveKind::Key => {
                    if first_measure || current.notes.is_empty() {
                        if let Some(ref kv) = dir.value {
                            let (f, m) = key_to_fifths(kv);
                            current.key = Some((f, m));
                        }
                    }
                }
                DirectiveKind::Tempo => {
                    if first_measure || current.notes.is_empty() {
                        if let Some(ref tv) = dir.value {
                            current.tempo = Some(tv.clone());
                        }
                    }
                }
                _ => {}
            },

            Line::Empty | Line::Comment(_, _) => {}
        }
    }

    // Flush last measure
    if !current.notes.is_empty() || first_measure {
        if let Some(label) = section_name {
            current.section_label = Some(label);
        }
        measures.push(current);
    }

    // Ensure at least one measure exists
    if measures.is_empty() {
        measures.push(Measure::default());
    }

    measures
}

/// Write a single measure to the output string.
fn write_measure(measure: &Measure, number: usize, out: &mut String) {
    out.push_str(&format!("    <measure number=\"{}\">\n", number));

    // Attributes (key + time signature + divisions) in first measure
    if measure.key.is_some() || number == 1 {
        out.push_str("      <attributes>\n");
        out.push_str("        <divisions>1</divisions>\n");
        if let Some((fifths, mode)) = measure.key {
            out.push_str("        <key>\n");
            out.push_str(&format!("          <fifths>{}</fifths>\n", fifths));
            out.push_str(&format!("          <mode>{}</mode>\n", mode));
            out.push_str("        </key>\n");
        }
        if number == 1 {
            out.push_str("        <time><beats>4</beats><beat-type>4</beat-type></time>\n");
            out.push_str("        <clef><sign>G</sign><line>2</line></clef>\n");
        }
        out.push_str("      </attributes>\n");
    }

    // Tempo direction
    if let Some(ref tempo) = measure.tempo {
        out.push_str("      <direction placement=\"above\">\n");
        out.push_str("        <direction-type>\n");
        out.push_str(&format!(
            "          <metronome><beat-unit>quarter</beat-unit><per-minute>{}</per-minute></metronome>\n",
            xml_escape(tempo)
        ));
        out.push_str("        </direction-type>\n");
        out.push_str(&format!(
            "        <sound tempo=\"{}\"/>\n",
            xml_escape(tempo)
        ));
        out.push_str("      </direction>\n");
    }

    // Rehearsal mark (section label)
    if let Some(ref label) = measure.section_label {
        out.push_str("      <direction placement=\"above\">\n");
        out.push_str("        <direction-type>\n");
        out.push_str("          <rehearsal>");
        xml_text(label, out);
        out.push_str("</rehearsal>\n");
        out.push_str("        </direction-type>\n");
        out.push_str("      </direction>\n");
    }

    // Notes
    for (chord_name, lyric_text) in &measure.notes {
        // Harmony element
        if let Some(chord) = chord_name {
            if let Some((root_step, root_alter, kind_content, kind_text, bass)) =
                chord_to_musicxml(chord)
            {
                out.push_str("      <harmony>\n");
                out.push_str("        <root>\n");
                out.push_str(&format!(
                    "          <root-step>{}</root-step>\n",
                    xml_escape(root_step)
                ));
                if root_alter != 0 {
                    out.push_str(&format!(
                        "          <root-alter>{}</root-alter>\n",
                        root_alter
                    ));
                }
                out.push_str("        </root>\n");
                out.push_str(&format!(
                    "        <kind text=\"{}\">{}</kind>\n",
                    xml_escape(kind_text),
                    xml_escape(kind_content)
                ));
                if let Some((bass_step, bass_alter)) = bass {
                    out.push_str("        <bass>\n");
                    out.push_str(&format!(
                        "          <bass-step>{}</bass-step>\n",
                        xml_escape(bass_step)
                    ));
                    if bass_alter != 0 {
                        out.push_str(&format!(
                            "          <bass-alter>{}</bass-alter>\n",
                            bass_alter
                        ));
                    }
                    out.push_str("        </bass>\n");
                }
                out.push_str("      </harmony>\n");
            }
        }

        // Note element (whole note)
        let lyric_trimmed = lyric_text.trim();
        out.push_str("      <note>\n");
        out.push_str("        <pitch><step>C</step><octave>4</octave></pitch>\n");
        out.push_str("        <duration>4</duration>\n");
        out.push_str("        <type>whole</type>\n");
        if !lyric_trimmed.is_empty() {
            out.push_str("        <lyric number=\"1\">\n");
            out.push_str("          <syllabic>single</syllabic>\n");
            out.push_str("          <text>");
            // Strip trailing hyphen that was added for syllabic continuation
            let display_text = lyric_trimmed.trim_end_matches('-');
            xml_text(display_text, out);
            out.push_str("</text>\n");
            out.push_str("        </lyric>\n");
        }
        out.push_str("      </note>\n");
    }

    out.push_str("    </measure>\n");
}

// ---------------------------------------------------------------------------
// Chord encoding
// ---------------------------------------------------------------------------

/// Convert a ChordPro chord name to MusicXML components.
///
/// Returns `(root_step, root_alter, kind_content, kind_text, bass)` or `None`
/// if the chord cannot be parsed.
///
/// - `root_step`: note letter ("C", "D", ..., "B")
/// - `root_alter`: semitone offset (1 = sharp, -1 = flat, 0 = natural)
/// - `kind_content`: MusicXML kind element text content ("major", "minor", ...)
/// - `kind_text`: the `text` attribute for the kind element (ChordPro suffix)
/// - `bass`: optional `(bass_step, bass_alter)` for slash chords
#[allow(clippy::type_complexity)]
fn chord_to_musicxml(
    chord_name: &str,
) -> Option<(
    &'static str,
    i32,
    &'static str,
    &'static str,
    Option<(&'static str, i32)>,
)> {
    let detail = parse_chord(chord_name)?;

    let root_step: &'static str = note_to_step(detail.root);
    let root_alter = acciental_to_alter(detail.root_accidental);

    let ext = detail.extension.as_deref().unwrap_or("");
    let (kind_content, kind_text) = quality_ext_to_kind(detail.quality, ext);

    let bass = if let Some((bass_note, bass_acc)) = detail.bass_note {
        let bs: &'static str = note_to_step(bass_note);
        let ba = acciental_to_alter(bass_acc);
        Some((bs, ba))
    } else {
        None
    };

    Some((root_step, root_alter, kind_content, kind_text, bass))
}

fn note_to_step(note: chordsketch_core::chord::Note) -> &'static str {
    use chordsketch_core::chord::Note;
    match note {
        Note::C => "C",
        Note::D => "D",
        Note::E => "E",
        Note::F => "F",
        Note::G => "G",
        Note::A => "A",
        Note::B => "B",
    }
}

fn acciental_to_alter(acc: Option<chordsketch_core::chord::Accidental>) -> i32 {
    use chordsketch_core::chord::Accidental;
    match acc {
        Some(Accidental::Sharp) => 1,
        Some(Accidental::Flat) => -1,
        _ => 0,
    }
}

/// Map (quality, extension) → (kind_content, kind_text_attr).
fn quality_ext_to_kind(
    quality: chordsketch_core::chord::ChordQuality,
    ext: &str,
) -> (&'static str, &'static str) {
    use chordsketch_core::chord::ChordQuality;

    match (quality, ext) {
        (ChordQuality::Major, "") => ("major", ""),
        (ChordQuality::Minor, "") => ("minor", "m"),
        (ChordQuality::Major, "7") => ("dominant", "7"),
        (ChordQuality::Major, "maj7") | (ChordQuality::Major, "M7") => ("major-seventh", "maj7"),
        (ChordQuality::Minor, "7") => ("minor-seventh", "m7"),
        (ChordQuality::Diminished, "") => ("diminished", "dim"),
        (ChordQuality::Diminished, "7") => ("diminished-seventh", "dim7"),
        (ChordQuality::Augmented, "") => ("augmented", "aug"),
        (ChordQuality::Major, "m7b5") | (ChordQuality::Minor, "7b5") => ("half-diminished", "m7b5"),
        (ChordQuality::Major, "6") => ("major-sixth", "6"),
        (ChordQuality::Minor, "6") => ("minor-sixth", "m6"),
        (ChordQuality::Major, "9") => ("dominant-ninth", "9"),
        (ChordQuality::Major, "maj9") => ("major-ninth", "maj9"),
        (ChordQuality::Minor, "9") => ("minor-ninth", "m9"),
        (ChordQuality::Major, "sus4") => ("suspended-fourth", "sus4"),
        (ChordQuality::Major, "sus2") => ("suspended-second", "sus2"),
        (ChordQuality::Major, "11") => ("dominant-11th", "11"),
        (ChordQuality::Major, "13") => ("dominant-13th", "13"),
        (ChordQuality::Major, "5") => ("power", "5"),
        // Fall back to "other" with the raw extension as display text
        (ChordQuality::Major, e) => ("other", e_to_static(e)),
        (ChordQuality::Minor, e) => ("other", e_to_static(e)),
        (ChordQuality::Diminished, _) => ("diminished", "dim"),
        (ChordQuality::Augmented, _) => ("augmented", "aug"),
    }
}

/// Leak a string slice into a `&'static str`.
///
/// This is only called for chord extensions that are not covered by the
/// static table above. The leaked string is small and the number of distinct
/// extensions per song is bounded.
fn e_to_static(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

// ---------------------------------------------------------------------------
// Key encoding
// ---------------------------------------------------------------------------

/// Convert a ChordPro key string to a (fifths, mode) pair.
fn key_to_fifths(key: &str) -> (i32, &'static str) {
    let is_minor = key.ends_with('m') && key.len() > 1;
    let root = if is_minor { &key[..key.len() - 1] } else { key };

    let fifths = match root {
        "Cb" | "C♭" => -7,
        "Gb" | "G♭" => -6,
        "Db" | "D♭" => -5,
        "Ab" | "A♭" => -4,
        "Eb" | "E♭" => -3,
        "Bb" | "B♭" => -2,
        "F" => -1,
        "C" => 0,
        "G" => 1,
        "D" => 2,
        "A" => 3,
        "E" => 4,
        "B" => 5,
        "F#" | "F♯" => 6,
        "C#" | "C♯" => 7,
        _ => 0,
    };

    let mode = if is_minor { "minor" } else { "major" };
    (fifths, mode)
}

// ---------------------------------------------------------------------------
// XML escaping
// ---------------------------------------------------------------------------

/// Escape a string for use as XML text content, appending to `out`.
fn xml_text(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
}

/// Escape a string for use in an XML attribute value (double-quoted).
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chordsketch_core::ast::{Chord, LyricsLine, LyricsSegment};

    fn simple_song() -> Song {
        let mut song = Song::new();
        song.metadata.title = Some("Test Song".to_string());
        song.metadata.artists = vec!["Test Artist".to_string()];
        song.metadata.key = Some("C".to_string());
        song.metadata.tempo = Some("120".to_string());
        // One lyrics line: [C]Hello [G]world
        let mut ll = LyricsLine::new();
        ll.segments = vec![
            LyricsSegment::new(Some(Chord::new("C")), "Hello "),
            LyricsSegment::new(Some(Chord::new("G")), "world "),
        ];
        song.lines.push(chordsketch_core::ast::Line::Lyrics(ll));
        song
    }

    #[test]
    fn export_contains_title() {
        let xml = to_musicxml(&simple_song());
        assert!(xml.contains("<work-title>Test Song</work-title>"));
    }

    #[test]
    fn export_contains_creator() {
        let xml = to_musicxml(&simple_song());
        assert!(xml.contains(r#"type="composer""#));
        assert!(xml.contains("Test Artist"));
    }

    #[test]
    fn export_contains_key_and_tempo() {
        let xml = to_musicxml(&simple_song());
        assert!(xml.contains("<fifths>0</fifths>"));
        assert!(xml.contains(r#"tempo="120""#));
    }

    #[test]
    fn export_contains_harmonies() {
        let xml = to_musicxml(&simple_song());
        assert!(xml.contains("<root-step>C</root-step>"));
        assert!(xml.contains("<root-step>G</root-step>"));
    }

    #[test]
    fn export_contains_lyrics() {
        let xml = to_musicxml(&simple_song());
        assert!(xml.contains("<text>Hello</text>"));
        assert!(xml.contains("<text>world</text>"));
    }

    #[test]
    fn export_escapes_special_chars() {
        let mut song = Song::new();
        song.metadata.title = Some("Song & <Things>".to_string());
        let xml = to_musicxml(&song);
        assert!(xml.contains("Song &amp; &lt;Things&gt;"));
    }

    #[test]
    fn key_to_fifths_major() {
        assert_eq!(key_to_fifths("C"), (0, "major"));
        assert_eq!(key_to_fifths("G"), (1, "major"));
        assert_eq!(key_to_fifths("F"), (-1, "major"));
        assert_eq!(key_to_fifths("Bb"), (-2, "major"));
        assert_eq!(key_to_fifths("F#"), (6, "major"));
    }

    #[test]
    fn key_to_fifths_minor() {
        assert_eq!(key_to_fifths("Am"), (3, "minor"));
        assert_eq!(key_to_fifths("Em"), (4, "minor"));
    }
}
