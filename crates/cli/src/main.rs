//! ChordPro command-line tool.
//!
//! Parses `.cho` / `.chordpro` files and renders them to text, HTML, or PDF.

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

    /// Transpose all chords by N semitones (positive = up, negative = down).
    #[arg(short, long, default_value = "0")]
    transpose: i8,

    /// Load a custom configuration file (may be specified multiple times).
    #[arg(short = 'c', long = "config")]
    configs: Vec<String>,

    /// Set a config value at runtime (highest precedence). Format: key=value.
    #[arg(short = 'D', long = "define")]
    defines: Vec<String>,
}

/// Supported output formats.
#[derive(Clone, Debug, clap::ValueEnum)]
enum Format {
    /// Plain text with chords above lyrics.
    Text,
    /// Self-contained HTML5 document.
    Html,
    /// PDF document (A4, Helvetica).
    Pdf,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    // Build configuration: defaults → system → user → custom config files → defines
    let mut config = chordpro_core::config::Config::defaults();

    // Apply --config files in order
    for config_path in &cli.configs {
        match std::fs::read_to_string(config_path) {
            Ok(text) => match chordpro_core::config::Config::parse(&text) {
                Ok(overlay) => config = config.merge(overlay),
                Err(e) => {
                    eprintln!("error: {config_path}: {e}");
                    return ExitCode::FAILURE;
                }
            },
            Err(e) => {
                eprintln!("error: {config_path}: {e}");
                return ExitCode::FAILURE;
            }
        }
    }

    // Apply --define overrides (highest precedence)
    for define in &cli.defines {
        if !define.contains('=') {
            eprintln!("error: invalid --define syntax: {define} (expected key=value)");
            return ExitCode::FAILURE;
        }
        config = config.with_define(define);
    }

    // Config is now built but not yet used by renderers.
    // Future work will pass it to render functions.
    let _ = config;

    let mut combined_text = String::new();
    let mut combined_bytes: Vec<u8> = Vec::new();
    let is_binary = matches!(cli.format, Format::Pdf);
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

        let song = match chordpro_core::parse(&input) {
            Ok(song) => song,
            Err(e) => {
                eprintln!(
                    "error: {path}: parse error at line {} column {}: {}",
                    e.line(),
                    e.column(),
                    e.message
                );
                had_error = true;
                continue;
            }
        };

        if is_binary {
            // PDF: each file produces a separate PDF. For multiple files,
            // only the last one is written (PDF doesn't support concatenation).
            if !combined_bytes.is_empty() {
                eprintln!(
                    "warning: PDF output supports one file at a time; previous output discarded"
                );
            }
            combined_bytes = chordpro_render_pdf::render_song_with_transpose(&song, cli.transpose);
        } else {
            let text = match cli.format {
                Format::Text => {
                    chordpro_render_text::render_song_with_transpose(&song, cli.transpose)
                }
                Format::Html => {
                    chordpro_render_html::render_song_with_transpose(&song, cli.transpose)
                }
                Format::Pdf => unreachable!(),
            };
            if !combined_text.is_empty() {
                combined_text.push('\n');
            }
            combined_text.push_str(&text);
        }
    }

    if had_error && combined_text.is_empty() && combined_bytes.is_empty() {
        return ExitCode::FAILURE;
    }

    let write_result = if is_binary {
        write_bytes(&cli.output, &combined_bytes)
    } else {
        write_text(&cli.output, &combined_text)
    };

    if let Err(e) = write_result {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }

    if had_error {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Write text output to a file or stdout.
fn write_text(path: &Option<String>, content: &str) -> io::Result<()> {
    match path {
        Some(path) => fs::write(path, content),
        None => {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            handle.write_all(content.as_bytes())
        }
    }
}

/// Write binary output to a file or stdout.
fn write_bytes(path: &Option<String>, content: &[u8]) -> io::Result<()> {
    match path {
        Some(path) => fs::write(path, content),
        None => {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            handle.write_all(content)
        }
    }
}
