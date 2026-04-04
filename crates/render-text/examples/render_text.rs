//! Parse a ChordPro string and render it to plain text.
//!
//! Run with: `cargo run --example render_text -p chordpro-render-text`

fn main() {
    let input = "\
{title: Amazing Grace}
{subtitle: Traditional}

{start_of_verse: Verse 1}
[G]Amazing [G7]grace, how [C]sweet the [G]sound,
That [G]saved a [Em]wretch like [D]me.
{end_of_verse}
";

    let song = chordpro_core::parse(input).expect("parse failed");
    let text = chordpro_render_text::render_song(&song);

    println!("{text}");
}
