//! ChordPro command-line tool.
//!
//! Parses `.cho` / `.chordpro` files and renders them to plain text.

use std::fs;
use std::io::{self, Write};
use std::process::ExitCode;

use clap::Parser;

/// ChordPro file processor — parse and render ChordPro songs.
#[derive(Parser)]
#[command(name = "chordpro", version, about)]
struct Cli {
    /// Input ChordPro file(s) to process.
    #[arg(required = true)]
    files: Vec<String>,

    /// Write output to a file instead of stdout.
    #[arg(short, long)]
    output: Option<String>,

    /// Output format.
    #[arg(short, long, default_value = "text")]
    format: Format,
}

/// Supported output formats.
#[derive(Clone, Debug, clap::ValueEnum)]
enum Format {
    /// Plain text with chords above lyrics.
    Text,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let mut combined_output = String::new();
    let mut had_error = false;

    for path in &cli.files {
        let input = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) => {
                eprintln!("error: {path}: {e}");
                had_error = true;
                continue;
            }
        };

        match render(&cli.format, &input) {
            Ok(text) => {
                // Separate multiple files with a blank line.
                if !combined_output.is_empty() {
                    combined_output.push('\n');
                }
                combined_output.push_str(&text);
            }
            Err(e) => {
                eprintln!("error: {path}: {e}");
                had_error = true;
            }
        }
    }

    if had_error && combined_output.is_empty() {
        return ExitCode::FAILURE;
    }

    if let Err(e) = write_output(&cli.output, &combined_output) {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }

    if had_error {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Render input using the selected format.
fn render(format: &Format, input: &str) -> Result<String, String> {
    match format {
        Format::Text => chordpro_render_text::try_render(input).map_err(|e| {
            format!(
                "parse error at line {} column {}: {}",
                e.line(),
                e.column(),
                e.message
            )
        }),
    }
}

/// Write output to a file or stdout.
fn write_output(path: &Option<String>, content: &str) -> io::Result<()> {
    match path {
        Some(path) => fs::write(path, content),
        None => {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            handle.write_all(content.as_bytes())
        }
    }
}
