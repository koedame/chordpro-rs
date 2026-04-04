//! Parse a multi-song ChordPro file and list each song's title.
//!
//! Run with: `cargo run --example multi_song -p chordpro-core`

fn main() {
    let input = "\
{title: Song One}
[C]First song [G]lyrics.

{new_song}

{title: Song Two}
[Am]Second song [F]lyrics.

{new_song}

{title: Song Three}
[G]Third song [D]lyrics.
";

    let songs = chordpro_core::parse_multi(input).expect("parse failed");

    println!("Found {} songs:", songs.len());
    for (i, song) in songs.iter().enumerate() {
        println!(
            "  {}. {}",
            i + 1,
            song.metadata.title.as_deref().unwrap_or("(untitled)")
        );
    }
}
