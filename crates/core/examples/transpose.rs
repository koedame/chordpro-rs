//! Parse a ChordPro string and transpose all chords up by 2 semitones.
//!
//! Run with: `cargo run --example transpose -p chordsketch-core`

use chordsketch_core::ast::Line;
use chordsketch_core::transpose::transpose;

fn main() {
    let input = "\
{title: Transposition Demo}

[C]Row, row, [G]row your [Am]boat,
[F]Gently [G]down the [C]stream.
";

    let song = chordsketch_core::parse(input).expect("parse failed");
    let transposed = transpose(&song, 2);

    println!("Original:");
    print_chords(&song.lines);
    println!();
    println!("After transposing +2 semitones:");
    print_chords(&transposed.lines);
}

fn print_chords(lines: &[Line]) {
    for line in lines {
        if let Line::Lyrics(lyrics) = line {
            for seg in &lyrics.segments {
                if let Some(chord) = &seg.chord {
                    print!("[{}]{}", chord.display_name(), seg.text);
                } else {
                    print!("{}", seg.text);
                }
            }
            println!();
        }
    }
}
