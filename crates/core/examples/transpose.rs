//! Parse a ChordPro string and transpose all chords up by 2 semitones.
//!
//! Run with: `cargo run --example transpose -p chordpro-core`

use chordpro_core::transpose::transpose;

fn main() {
    let input = "\
{title: Transposition Demo}

[C]Row, row, [G]row your [Am]boat,
[F]Gently [G]down the [C]stream.
";

    let song = chordpro_core::parse(input).expect("parse failed");
    let transposed = transpose(&song, 2);

    println!("Original key directives:");
    println!("  title = {:?}", song.metadata.title);
    println!();
    println!("After transposing +2 semitones:");
    println!("  title = {:?}", transposed.metadata.title);
    println!("  (chords shifted: C->D, G->A, Am->Bm, F->G)");
}
