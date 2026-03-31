//! ChordPro command-line tool.
//!
//! Parses `.cho` / `.chordpro` files and renders them to text, HTML, or PDF.

use std::fs;
use std::io::{self, Write};
use std::path::Path;
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

    /// Load a custom configuration file or preset name (may be specified multiple times).
    ///
    /// Paths are treated as trusted input — the file is read without path restrictions.
    /// Do not pass untrusted paths when invoking chordpro programmatically.
    #[arg(short = 'c', long = "config")]
    configs: Vec<String>,

    /// Set a config value at runtime (highest precedence). Format: key=value.
    #[arg(short = 'D', long = "define")]
    defines: Vec<String>,

    /// Skip loading system, user, and project config files.
    /// Only built-in defaults are used as the base. --config and --define still apply.
    #[arg(long = "no-default-configs")]
    no_default_configs: bool,
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

    // Derive project directory from the first input file for project-level config.
    let project_dir = cli
        .files
        .first()
        .and_then(|f| Path::new(f).parent())
        .and_then(|p| p.to_str())
        .map(|s| s.to_string());

    // Build configuration: defaults → system → user → project → custom config files → defines
    let mut config = if cli.no_default_configs {
        chordpro_core::config::Config::defaults()
    } else {
        chordpro_core::config::Config::load(project_dir.as_deref(), None)
    };

    // Apply --config files/presets in order (preset names resolved first)
    for config_name in &cli.configs {
        match chordpro_core::config::Config::resolve(config_name) {
            Ok(overlay) => config = config.merge(overlay),
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        }
    }

    // Auto-detect external delegate tools and enable them in config.
    // This runs after user configs but before --define, so explicit
    // --define delegates.abc2svg=false can still override.
    if chordpro_core::external_tool::has_abc2svg() {
        config = config.with_define("delegates.abc2svg=true");
    }
    if chordpro_core::external_tool::has_lilypond() {
        config = config.with_define("delegates.lilypond=true");
    }

    // Apply --define overrides (highest precedence)
    for define in &cli.defines {
        if !define.contains('=') {
            eprintln!("error: invalid --define syntax: {define} (expected key=value)");
            return ExitCode::FAILURE;
        }
        config = config.with_define(define);
    }

    let mut all_songs: Vec<chordpro_core::ast::Song> = Vec::new();
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

        let result = chordpro_core::parse_multi_lenient(&input);
        for parse_result in &result.results {
            for e in &parse_result.errors {
                eprintln!(
                    "error: {path}: parse error at line {} column {}: {}",
                    e.line(),
                    e.column(),
                    e.message
                );
                had_error = true;
            }
        }
        all_songs.extend(result.results.into_iter().map(|r| r.song));
    }

    let combined_text;
    let combined_bytes;

    if is_binary {
        combined_bytes =
            chordpro_render_pdf::render_songs_with_transpose(&all_songs, cli.transpose, &config);
        combined_text = String::new();
    } else {
        combined_bytes = Vec::new();
        combined_text = match cli.format {
            Format::Text => chordpro_render_text::render_songs_with_transpose(
                &all_songs,
                cli.transpose,
                &config,
            ),
            Format::Html => chordpro_render_html::render_songs_with_transpose(
                &all_songs,
                cli.transpose,
                &config,
            ),
            Format::Pdf => unreachable!(),
        };
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
