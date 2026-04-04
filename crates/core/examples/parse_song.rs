//! Parse a ChordPro string and print the song metadata and line count.
//!
//! Run with: `cargo run --example parse_song -p chordpro-core`

fn main() {
    let input = "\
{title: Amazing Grace}
{subtitle: Traditional}
{key: G}

{start_of_verse}
[G]Amazing [G7]grace, how [C]sweet the [G]sound,
That [G]saved a [Em]wretch like [D]me.
{end_of_verse}
";

    let song = chordpro_core::parse(input).expect("parse failed");

    println!("Title:    {:?}", song.metadata.title);
    println!("Subtitle: {:?}", song.metadata.subtitles);
    println!("Key:      {:?}", song.metadata.key);
    println!("Lines:    {}", song.lines.len());
}
