//! MusicXML → ChordPro importer.
//!
//! Converts a MusicXML 4.0 `<score-partwise>` document into a [`Song`] AST
//! by extracting chord symbols, lyrics, metadata, and section structure.
//!
//! # What is extracted
//!
//! - Song metadata: `<work-title>`, `<creator type="composer">`, key signature,
//!   tempo (`<sound tempo="...">`), capo (`<capo>`)
//! - Chord symbols: reconstructed from `<harmony>` elements using root step/alter
//!   and the kind `text` attribute (falling back to kind content mapping)
//! - Lyrics: from `<note><lyric>` elements; each lyric syllable is matched to
//!   the preceding harmony
//! - Section structure: rehearsal marks (`<rehearsal>`) are mapped to ChordPro
//!   start-of-verse / start-of-chorus / start-of-bridge directives
//!
//! # What is not extracted
//!
//! - Staff notation (notes, pitch, rhythm, duration)
//! - Multi-part scores (only the first `<part>` is used)
//! - Dynamics, articulations, slurs
//! - Chord diagrams

use crate::xml::{Element, parse};
use chordsketch_core::ast::{Chord, Directive, Line, LyricsLine, LyricsSegment, Song};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Error type for MusicXML import failures.
#[derive(Debug)]
pub struct ImportError(String);

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MusicXML import error: {}", self.0)
    }
}

impl std::error::Error for ImportError {}

/// Converts a MusicXML string into a [`Song`] AST.
///
/// Only the first `<part>` is used; additional parts are ignored. Chord
/// symbols are reconstructed from `<harmony>` elements; lyrics are extracted
/// from `<note><lyric>` elements and aligned to the preceding chord.
///
/// # Errors
///
/// Returns an [`ImportError`] if the XML cannot be parsed or if the document
/// is not a `<score-partwise>` MusicXML file.
pub fn from_musicxml(xml: &str) -> Result<Song, ImportError> {
    let root = parse(xml).map_err(|e| ImportError(format!("XML parse error: {e}")))?;
    if root.name != "score-partwise" {
        return Err(ImportError(format!(
            "expected <score-partwise> root element, found <{}>",
            root.name
        )));
    }
    convert_score(&root)
}

// ---------------------------------------------------------------------------
// Internal conversion
// ---------------------------------------------------------------------------

fn convert_score(score: &Element) -> Result<Song, ImportError> {
    let mut song = Song::new();

    // --- Metadata -----------------------------------------------------------

    // Title from <work><work-title>
    if let Some(title) = score.text_at(&["work", "work-title"]) {
        if !title.is_empty() {
            song.metadata.title = Some(title.to_string());
        }
    }

    // Creators from <identification><creator type="...">
    if let Some(ident) = score.child("identification") {
        for creator in ident.children_named("creator") {
            let kind = creator.attr("type").unwrap_or("");
            let name = creator.text.trim();
            if name.is_empty() {
                continue;
            }
            match kind {
                "composer" | "arranger" => song.metadata.artists.push(name.to_string()),
                "lyricist" => song.metadata.lyricists.push(name.to_string()),
                _ => {
                    // Store unknown creator types as custom metadata
                    song.metadata
                        .custom
                        .push((kind.to_string(), name.to_string()));
                }
            }
        }
    }

    // --- Body ---------------------------------------------------------------

    // Use the first <part> only
    let part = score
        .child("part")
        .ok_or_else(|| ImportError("no <part> element found".to_string()))?;

    let mut current_key: Option<String> = None;
    let mut key_emitted = false;
    let mut tempo_emitted = false;
    let mut capo_emitted = false;
    // Tracks the end-directive name of the currently open section, if any.
    let mut current_section_end: Option<&'static str> = None;

    for measure in part.children_named("measure") {
        // --- attributes (key, capo) -----------------------------------------
        if let Some(attrs) = measure.child("attributes") {
            if let Some(key_elem) = attrs.child("key") {
                let fifths: i32 = key_elem
                    .text_at(&["fifths"])
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                let mode = key_elem.text_at(&["mode"]).unwrap_or("major");
                let key_name = fifths_to_key(fifths, mode);
                current_key = Some(key_name);
            }
            if !capo_emitted {
                if let Some(capo_text) = attrs.text_at(&["capo"]) {
                    // Validate: capo must be a non-negative integer in [1, 24].
                    // Values outside this range are silently ignored — a capo
                    // of 0 means "no capo" and > 24 is beyond any real guitar fret.
                    if let Ok(capo_val) = capo_text.trim().parse::<u8>() {
                        if (1..=24).contains(&capo_val) {
                            emit_directive(&mut song.lines, "capo", Some(capo_text.trim()));
                            capo_emitted = true;
                        }
                    }
                }
            }
        }

        // Emit key once (first time we see it)
        if !key_emitted {
            if let Some(ref key) = current_key {
                if song.metadata.key.is_none() {
                    song.metadata.key = Some(key.clone());
                }
                key_emitted = true;
            }
        }

        // --- directions (tempo, rehearsal marks) ----------------------------
        for direction in measure.children_named("direction") {
            // Tempo: `<sound tempo="...">` inside `<direction>`.
            // Validate the value is a positive finite number before storing.
            if !tempo_emitted {
                if let Some(sound) = direction.child("sound") {
                    if let Some(tempo) = sound.attr("tempo") {
                        if let Ok(bpm) = tempo.trim().parse::<f64>() {
                            if bpm > 0.0 && bpm.is_finite() {
                                song.metadata.tempo = Some(tempo.trim().to_string());
                                tempo_emitted = true;
                            }
                        }
                    }
                }
            }

            // Rehearsal marks → section start directives
            if let Some(dt) = direction.child("direction-type") {
                if let Some(rehearsal) = dt.child("rehearsal") {
                    let label = rehearsal.text.trim();
                    if !label.is_empty() {
                        // Close any open section with its end directive.
                        if let Some(end_dir) = current_section_end {
                            emit_directive(&mut song.lines, end_dir, None);
                            song.lines.push(Line::Empty);
                        }
                        let (section_dir, section_end) = map_section_label(label);
                        emit_directive(&mut song.lines, section_dir, Some(label));
                        current_section_end = Some(section_end);
                    }
                }
                if let Some(words) = dt.child("words") {
                    let text = words.text.trim();
                    // Treat "Intro:", "Verse", "Chorus", etc. as section markers
                    if looks_like_section_marker(text) {
                        // Close any open section with its end directive.
                        if let Some(end_dir) = current_section_end {
                            emit_directive(&mut song.lines, end_dir, None);
                            song.lines.push(Line::Empty);
                        }
                        let (section_dir, section_end) = map_section_label(text);
                        emit_directive(&mut song.lines, section_dir, Some(text));
                        current_section_end = Some(section_end);
                    }
                }
            }
        }

        // --- collect chords and lyrics per measure --------------------------
        let segments = collect_measure_segments(measure);
        if segments.is_empty() {
            continue;
        }

        // Build LyricsLine(s) from segments
        // Each segment is (Option<chord_name>, lyric_text)
        let mut line_segs: Vec<LyricsSegment> = Vec::new();
        for (chord_name, lyric_text) in segments {
            let chord = chord_name.map(Chord::new);
            line_segs.push(LyricsSegment::new(chord, lyric_text));
        }

        if !line_segs.is_empty() {
            let mut ll = LyricsLine::new();
            ll.segments = line_segs;
            song.lines.push(Line::Lyrics(ll));
        }
    }

    // Close the last open section with its end directive.
    if let Some(end_dir) = current_section_end {
        emit_directive(&mut song.lines, end_dir, None);
    }

    Ok(song)
}

/// Collect (chord_name, lyric_text) pairs from a measure.
///
/// Chords are matched to the lyric that immediately follows them. Notes
/// without a lyric advance the "has chord available" state so the chord
/// floats to the next lyric-bearing note.
fn collect_measure_segments(measure: &Element) -> Vec<(Option<String>, String)> {
    let mut result: Vec<(Option<String>, String)> = Vec::new();
    let mut pending_chord: Option<String> = None;

    for child in &measure.children {
        match child.name.as_str() {
            "harmony" => {
                pending_chord = parse_harmony(child);
            }
            "note" => {
                // Skip chord notes (notes that are part of a chord voicing)
                if child.child("chord").is_some() {
                    continue;
                }

                let lyric_text = collect_lyric_text(child);
                if !lyric_text.is_empty() {
                    result.push((pending_chord.take(), lyric_text));
                } else {
                    // Note has no lyric — keep the chord pending for the next lyric
                }
            }
            _ => {}
        }
    }

    // If there's a leftover pending chord with no following lyric, add it
    // as a chord-only segment so it isn't lost.
    if let Some(chord) = pending_chord {
        result.push((Some(chord), String::new()));
    }

    result
}

/// Extract the chord name from a `<harmony>` element.
fn parse_harmony(harmony: &Element) -> Option<String> {
    let root = harmony.child("root")?;
    let step = root.text_at(&["root-step"])?;
    if step.is_empty() {
        return None;
    }

    let alter = root
        .text_at(&["root-alter"])
        .and_then(|s| {
            let v: f64 = s.trim().parse().ok()?;
            Some(v)
        })
        .unwrap_or(0.0);

    let root_str = format!(
        "{}{}",
        step,
        match alter as i32 {
            1 => "#",
            -1 => "b",
            _ => "",
        }
    );

    // Try the `text` attribute on <kind> first — it often has the display
    // chord symbol (e.g., "m7", "maj7", "sus4").
    let suffix = if let Some(kind_elem) = harmony.child("kind") {
        if let Some(text_attr) = kind_elem.attr("text") {
            // text="" means major, text="m" means minor, etc.
            text_attr.to_string()
        } else {
            // Fall back to mapping the kind content
            let kind_content = kind_elem.text.trim();
            musicxml_kind_to_suffix(kind_content).to_string()
        }
    } else {
        String::new()
    };

    // Bass note for slash chords
    let bass_str = if let Some(bass) = harmony.child("bass") {
        let bass_step = bass.text_at(&["bass-step"]).unwrap_or("");
        if bass_step.is_empty() {
            String::new()
        } else {
            let bass_alter = bass
                .text_at(&["bass-alter"])
                .and_then(|s| s.trim().parse::<f64>().ok())
                .unwrap_or(0.0);
            format!(
                "/{}{}",
                bass_step,
                match bass_alter as i32 {
                    1 => "#",
                    -1 => "b",
                    _ => "",
                }
            )
        }
    } else {
        String::new()
    };

    Some(format!("{root_str}{suffix}{bass_str}"))
}

/// Map MusicXML `<kind>` content to a ChordPro chord suffix.
fn musicxml_kind_to_suffix(kind: &str) -> &str {
    match kind {
        "major" | "none" | "" => "",
        "minor" => "m",
        "dominant" => "7",
        "major-seventh" => "maj7",
        "minor-seventh" => "m7",
        "diminished" => "dim",
        "augmented" => "aug",
        "half-diminished" => "m7b5",
        "diminished-seventh" => "dim7",
        "major-sixth" => "6",
        "minor-sixth" => "m6",
        "dominant-ninth" => "9",
        "major-ninth" => "maj9",
        "minor-ninth" => "m9",
        "suspended-fourth" => "sus4",
        "suspended-second" => "sus2",
        "dominant-11th" => "11",
        "major-11th" => "maj11",
        "dominant-13th" => "13",
        "major-13th" => "maj13",
        "power" => "5",
        "major-minor" => "mM7",
        "augmented-seventh" => "aug7",
        "augmented-major-seventh" => "augM7",
        _ => "",
    }
}

/// Collect the lyric text from a `<note>` element.
///
/// A single MusicXML `<note>` may carry multiple `<lyric>` children, one
/// per verse (e.g. `<lyric number="1">…</lyric><lyric number="2">…</lyric>`).
/// Every non-empty verse is folded into the returned string, separated
/// by `" / "` so that multi-verse hymns round-trip through ChordPro
/// without silently dropping verses 2..N (issue #1829).
///
/// The `syllabic` element of each verse is preserved independently —
/// `begin` / `middle` produce a trailing hyphen; `end` / `single` (and
/// anything else) produce a trailing space.
///
/// Returns the empty string if no verse has non-empty text.
fn collect_lyric_text(note: &Element) -> String {
    let mut verses: Vec<String> = Vec::new();
    for lyric in note.children_named("lyric") {
        let text = lyric.text_at(&["text"]).unwrap_or("").trim();
        if text.is_empty() {
            continue;
        }
        let syllabic = lyric.text_at(&["syllabic"]).unwrap_or("single");
        let chunk = match syllabic {
            "begin" | "middle" => format!("{text}-"),
            _ => format!("{text} "),
        };
        verses.push(chunk);
    }
    // `join` is intentional even for the single-verse case: the trailing
    // space or hyphen from `chunk` above is preserved and the delimiter
    // only appears between verses, which is the behaviour existing
    // single-verse callers expect.
    verses.join(" / ")
}

/// Map a key signature (circle-of-fifths value + mode) to a key name string.
fn fifths_to_key(fifths: i32, mode: &str) -> String {
    let major_keys = [
        "Cb", "Gb", "Db", "Ab", "Eb", "Bb", "F", "C", "G", "D", "A", "E", "B", "F#", "C#",
    ];
    // Index 7 = C (fifths=0), offset by +7
    let idx = (fifths + 7).clamp(0, 14) as usize;
    let key = major_keys[idx];
    if mode.eq_ignore_ascii_case("minor") {
        format!("{key}m")
    } else {
        key.to_string()
    }
}

/// Decide which section directive to emit for a rehearsal/words label.
///
/// Returns `(start_directive_name, end_directive_name)`.
fn map_section_label(label: &str) -> (&'static str, &'static str) {
    let lower = label.to_lowercase();
    if lower.contains("chorus") || lower.contains("refrain") {
        ("start_of_chorus", "end_of_chorus")
    } else if lower.contains("bridge") {
        ("start_of_bridge", "end_of_bridge")
    } else if lower.contains("intro") || lower.contains("outro") {
        ("start_of_verse", "end_of_verse")
    } else {
        // Default to verse
        ("start_of_verse", "end_of_verse")
    }
}

/// Heuristic: is this a words string likely to be a section label?
fn looks_like_section_marker(text: &str) -> bool {
    let lower = text.to_lowercase();
    matches!(
        lower.as_str(),
        "verse"
            | "chorus"
            | "bridge"
            | "intro"
            | "outro"
            | "pre-chorus"
            | "prechorus"
            | "interlude"
            | "coda"
            | "tag"
    ) || lower.starts_with("verse ")
        || lower.starts_with("chorus ")
}

/// Push a directive line onto `lines`.
fn emit_directive(lines: &mut Vec<Line>, name: &str, value: Option<&str>) {
    let dir = match value {
        Some(v) => Directive::with_value(name, v),
        None => Directive::name_only(name),
    };
    lines.push(Line::Directive(dir));
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifths_to_key_major() {
        assert_eq!(fifths_to_key(0, "major"), "C");
        assert_eq!(fifths_to_key(1, "major"), "G");
        assert_eq!(fifths_to_key(-1, "major"), "F");
        assert_eq!(fifths_to_key(2, "major"), "D");
        assert_eq!(fifths_to_key(-2, "major"), "Bb");
        assert_eq!(fifths_to_key(4, "major"), "E");
    }

    #[test]
    fn fifths_to_key_minor() {
        assert_eq!(fifths_to_key(0, "minor"), "Cm");
        assert_eq!(fifths_to_key(3, "minor"), "Am");
    }

    #[test]
    fn musicxml_kind_suffix() {
        assert_eq!(musicxml_kind_to_suffix("major"), "");
        assert_eq!(musicxml_kind_to_suffix("minor"), "m");
        assert_eq!(musicxml_kind_to_suffix("dominant"), "7");
        assert_eq!(musicxml_kind_to_suffix("major-seventh"), "maj7");
        assert_eq!(musicxml_kind_to_suffix("diminished"), "dim");
        assert_eq!(musicxml_kind_to_suffix("suspended-fourth"), "sus4");
    }

    #[test]
    fn simple_import() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="4.0">
  <work><work-title>Test Song</work-title></work>
  <identification>
    <creator type="composer">Test Artist</creator>
  </identification>
  <part-list>
    <score-part id="P1"><part-name>Voice</part-name></score-part>
  </part-list>
  <part id="P1">
    <measure number="1">
      <attributes>
        <key><fifths>0</fifths><mode>major</mode></key>
      </attributes>
      <direction><sound tempo="120"/></direction>
      <harmony>
        <root><root-step>C</root-step></root>
        <kind text="">major</kind>
      </harmony>
      <note>
        <duration>1</duration>
        <lyric><syllabic>single</syllabic><text>Hello</text></lyric>
      </note>
      <harmony>
        <root><root-step>G</root-step></root>
        <kind text="">major</kind>
      </harmony>
      <note>
        <duration>1</duration>
        <lyric><syllabic>single</syllabic><text>world</text></lyric>
      </note>
    </measure>
  </part>
</score-partwise>"#;
        let song = from_musicxml(xml).unwrap();
        assert_eq!(song.metadata.title.as_deref(), Some("Test Song"));
        assert_eq!(
            song.metadata.artists.first().map(String::as_str),
            Some("Test Artist")
        );
        assert_eq!(song.metadata.key.as_deref(), Some("C"));
        assert_eq!(song.metadata.tempo.as_deref(), Some("120"));

        // Should have one lyrics line with two segments
        let lyrics: Vec<&Line> = song
            .lines
            .iter()
            .filter(|l| matches!(l, Line::Lyrics(_)))
            .collect();
        assert_eq!(lyrics.len(), 1);
        if let Line::Lyrics(ll) = lyrics[0] {
            assert_eq!(ll.segments.len(), 2);
            assert_eq!(
                ll.segments[0].chord.as_ref().map(|c| c.name.as_str()),
                Some("C")
            );
            assert!(ll.segments[0].text.contains("Hello"));
            assert_eq!(
                ll.segments[1].chord.as_ref().map(|c| c.name.as_str()),
                Some("G")
            );
            assert!(ll.segments[1].text.contains("world"));
        }
    }

    #[test]
    fn import_sharp_flat_chords() {
        let xml = r#"<score-partwise version="4.0">
  <part-list><score-part id="P1"><part-name/></score-part></part-list>
  <part id="P1">
    <measure number="1">
      <harmony>
        <root><root-step>F</root-step><root-alter>1</root-alter></root>
        <kind text="m">minor</kind>
      </harmony>
      <note>
        <duration>1</duration>
        <lyric><syllabic>single</syllabic><text>hi</text></lyric>
      </note>
      <harmony>
        <root><root-step>B</root-step><root-alter>-1</root-alter></root>
        <kind text="">major</kind>
      </harmony>
      <note>
        <duration>1</duration>
        <lyric><syllabic>single</syllabic><text>there</text></lyric>
      </note>
    </measure>
  </part>
</score-partwise>"#;
        let song = from_musicxml(xml).unwrap();
        let lyrics: Vec<&Line> = song
            .lines
            .iter()
            .filter(|l| matches!(l, Line::Lyrics(_)))
            .collect();
        assert_eq!(lyrics.len(), 1);
        if let Line::Lyrics(ll) = lyrics[0] {
            assert_eq!(
                ll.segments[0].chord.as_ref().map(|c| c.name.as_str()),
                Some("F#m")
            );
            assert_eq!(
                ll.segments[1].chord.as_ref().map(|c| c.name.as_str()),
                Some("Bb")
            );
        }
    }

    #[test]
    fn import_multi_verse_lyrics() {
        // #1829: a note with <lyric number="1"> and <lyric number="2">
        // used to silently drop verse 2. `collect_lyric_text` now folds
        // every non-empty verse into the returned lyric, joined by
        // " / ", so the ChordPro round-trip does not lose verses.
        let xml = r#"<score-partwise version="4.0">
  <part-list><score-part id="P1"><part-name/></score-part></part-list>
  <part id="P1">
    <measure number="1">
      <harmony>
        <root><root-step>G</root-step></root>
        <kind text="">major</kind>
      </harmony>
      <note>
        <duration>1</duration>
        <lyric number="1"><syllabic>single</syllabic><text>Amazing</text></lyric>
        <lyric number="2"><syllabic>single</syllabic><text>'Twas</text></lyric>
      </note>
    </measure>
  </part>
</score-partwise>"#;
        let song = from_musicxml(xml).unwrap();
        let lyrics: Vec<&Line> = song
            .lines
            .iter()
            .filter(|l| matches!(l, Line::Lyrics(_)))
            .collect();
        assert_eq!(lyrics.len(), 1);
        if let Line::Lyrics(ll) = lyrics[0] {
            assert_eq!(ll.segments.len(), 1);
            let text = &ll.segments[0].text;
            assert!(
                text.contains("Amazing") && text.contains("'Twas"),
                "expected both verses in joined lyric text; got {text:?}"
            );
            assert!(
                text.contains(" / "),
                "expected ' / ' delimiter between verses; got {text:?}"
            );
        }
    }

    #[test]
    fn collect_lyric_text_single_verse_preserves_trailing_space() {
        // Regression guard: single-verse callers must keep the trailing
        // space / hyphen that the syllabic branch emits, even now that
        // the implementation folds through `join`.
        let xml = r#"<note>
  <lyric number="1"><syllabic>single</syllabic><text>hi</text></lyric>
</note>"#;
        let element = crate::xml::parse(xml).unwrap();
        assert_eq!(collect_lyric_text(&element), "hi ");
    }

    #[test]
    fn collect_lyric_text_begin_syllabic_produces_hyphen() {
        let xml = r#"<note>
  <lyric number="1"><syllabic>begin</syllabic><text>a</text></lyric>
</note>"#;
        let element = crate::xml::parse(xml).unwrap();
        assert_eq!(collect_lyric_text(&element), "a-");
    }

    #[test]
    fn collect_lyric_text_skips_empty_verses() {
        // Verse 1 empty, verse 2 non-empty → verse 2 alone.
        let xml = r#"<note>
  <lyric number="1"><syllabic>single</syllabic><text></text></lyric>
  <lyric number="2"><syllabic>single</syllabic><text>ok</text></lyric>
</note>"#;
        let element = crate::xml::parse(xml).unwrap();
        assert_eq!(collect_lyric_text(&element), "ok ");
    }
}
