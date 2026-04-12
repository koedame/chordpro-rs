//! ChordPro command-line tool.
//!
//! Parses `.cho` / `.chordpro` files and renders them to text, HTML, or PDF.
//! Also provides `chordsketch fmt` for formatting ChordPro source files.

use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitCode;

use tempfile::NamedTempFile;

/// Maximum input size accepted at the I/O layer (50 MiB), matching the
/// parser-level guard in `chordsketch-convert-musicxml`. Files or stdin
/// streams larger than this limit are rejected before the content is fully
/// buffered into memory.
const MAX_INPUT_BYTES: u64 = 52_428_800;

/// Reads `path` into a [`String`], returning an error string if the file is
/// larger than [`MAX_INPUT_BYTES`] or if any I/O error occurs.
///
/// [`fs::metadata`] is used for an early, cheap size check that avoids
/// loading oversized files into memory at all.
fn read_file_clamped(path: &str) -> Result<String, String> {
    match fs::metadata(path) {
        Ok(meta) if meta.len() > MAX_INPUT_BYTES => {
            return Err(format!(
                "file exceeds the {} MiB input limit",
                MAX_INPUT_BYTES / (1024 * 1024)
            ));
        }
        Err(e) => return Err(e.to_string()),
        Ok(_) => {}
    }
    fs::read_to_string(path).map_err(|e| e.to_string())
}

/// Reads stdin into a [`String`], rejecting streams that exceed
/// [`MAX_INPUT_BYTES`].
///
/// Uses [`Read::take`] to avoid buffering more than `MAX_INPUT_BYTES + 1`
/// bytes; if the read fills the cap the stream is rejected without consuming
/// the remainder.
fn read_stdin_clamped() -> Result<String, String> {
    let mut buf = String::new();
    io::stdin()
        .take(MAX_INPUT_BYTES + 1)
        .read_to_string(&mut buf)
        .map_err(|e| e.to_string())?;
    if buf.len() as u64 > MAX_INPUT_BYTES {
        return Err(format!(
            "input exceeds the {} MiB input limit",
            MAX_INPUT_BYTES / (1024 * 1024)
        ));
    }
    Ok(buf)
}

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

/// ChordPro file processor — parse and render ChordPro songs.
#[derive(Parser)]
#[command(name = "chordsketch", version, about)]
#[command(args_conflicts_with_subcommands = true)]
struct Cli {
    /// Subcommand (e.g. `fmt`).
    #[command(subcommand)]
    command: Option<Commands>,

    /// Input ChordPro file(s) to process.
    #[arg(required = false)]
    files: Vec<String>,

    /// Generate shell completions and print to stdout.
    #[arg(long, value_name = "SHELL")]
    completions: Option<Shell>,

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

    /// Set the active instrument for selector filtering.
    ///
    /// Directives with a selector suffix (e.g., `{textfont-piano: Courier}`)
    /// are kept only when the selector matches the active instrument or user.
    /// Equivalent to `--define instrument.type=<INSTRUMENT>`.
    #[arg(long)]
    instrument: Option<String>,
}

/// Available subcommands.
#[derive(Subcommand)]
enum Commands {
    /// Format ChordPro source files.
    ///
    /// Normalize directive names, spacing, chord spelling, and blank lines.
    /// Use '-' as a file name to read from stdin and write to stdout.
    /// With --check the files are not modified; exits 1 if any needs formatting.
    Fmt {
        /// Files to format. Use '-' to read from stdin and write to stdout.
        #[arg(required = true)]
        files: Vec<String>,

        /// Check only — do not modify files. Exit 1 if any file is not formatted.
        #[arg(long)]
        check: bool,
    },

    /// Convert between ChordPro and other music notation formats.
    ///
    /// Import: reads plain-text, ABC notation (`.abc`), or MusicXML (`.xml`)
    /// files and converts them to ChordPro (`.cho`) format.
    ///
    /// Export: reads ChordPro files and converts them to MusicXML (`.xml`)
    /// using `--to musicxml`.
    ///
    /// Auto-detection is used by default for import.  Pass `--from plaintext`,
    /// `--from abc`, or `--from musicxml` to force a specific input format.
    Convert {
        /// Input file(s) to convert. Use '-' to read from stdin.
        #[arg(required = true)]
        files: Vec<String>,

        /// Input format. `auto` detects the format automatically;
        /// `plaintext` forces plain chord+lyrics conversion;
        /// `abc` forces ABC notation conversion;
        /// `musicxml` forces MusicXML import.
        #[arg(long, default_value = "auto")]
        from: ConvertFrom,

        /// Output format for export. `musicxml` converts ChordPro → MusicXML.
        /// When omitted, the output is ChordPro (`.cho`) format.
        #[arg(long)]
        to: Option<ConvertTo>,

        /// Write output to a file instead of stdout.
        #[arg(short, long)]
        output: Option<String>,
    },
}

/// Input format for the `convert` subcommand.
#[derive(Clone, Debug, clap::ValueEnum)]
enum ConvertFrom {
    /// Automatically detect the input format from file extension and content.
    Auto,
    /// Force plain chord+lyrics conversion.
    Plaintext,
    /// Force ABC notation conversion.
    Abc,
    /// Force MusicXML import.
    Musicxml,
}

/// Output format for the `convert --to` flag.
#[derive(Clone, Debug, clap::ValueEnum)]
enum ConvertTo {
    /// Export to MusicXML 4.0.
    Musicxml,
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

    // Dispatch subcommands.
    if let Some(Commands::Fmt { files, check }) = cli.command {
        return run_fmt(&files, check);
    }
    if let Some(Commands::Convert {
        files,
        from,
        to,
        output,
    }) = cli.command
    {
        return run_convert(&files, from, to, output.as_deref());
    }

    // Render mode: require at least one file (unless generating completions).
    if cli.files.is_empty() && cli.completions.is_none() {
        // Reuse clap's own error formatting for a consistent user experience.
        let mut cmd = Cli::command();
        cmd.error(
            clap::error::ErrorKind::MissingRequiredArgument,
            "<FILES> required for render mode",
        )
        .exit();
    }

    if let Some(shell) = cli.completions {
        let mut cmd = Cli::command();
        clap_complete::generate(shell, &mut cmd, "chordsketch", &mut io::stdout());
        return ExitCode::SUCCESS;
    }

    // Derive project directory from the first input file for project-level config.
    let project_dir = cli
        .files
        .first()
        .and_then(|f| Path::new(f).parent())
        .and_then(|p| p.to_str())
        .map(|s| s.to_string());

    // Build configuration: defaults → system → user → project → custom config files → defines
    let mut config = if cli.no_default_configs {
        chordsketch_core::config::Config::defaults()
    } else {
        let result = chordsketch_core::config::Config::load(project_dir.as_deref(), None);
        for warning in &result.warnings {
            eprintln!("warning: {warning}");
        }
        result.config
    };

    // Apply --config files/presets in order (preset names resolved first)
    for config_name in &cli.configs {
        match chordsketch_core::config::Config::resolve(config_name) {
            Ok(result) => {
                for warning in &result.warnings {
                    eprintln!("warning: {config_name}: {warning}");
                }
                config = config.merge(result.config);
            }
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        }
    }

    // Apply --instrument shorthand before --define so that --define can override.
    // Empty/whitespace-only values are ignored — they would silently remove all
    // selector-bearing directives since no selector matches the empty string.
    if let Some(ref instrument) = cli.instrument {
        if !instrument.trim().is_empty() {
            config = config
                .with_define(&format!("instrument.type={instrument}"))
                .expect("instrument.type define is valid");
        }
    }

    // Apply --define overrides (highest precedence)
    for define in &cli.defines {
        match config.with_define(define) {
            Ok(updated) => config = updated,
            Err(e) => {
                eprintln!("error: invalid --define: {define} ({e})");
                return ExitCode::FAILURE;
            }
        }
    }

    // Combine CLI --transpose with settings.transpose from config.
    // CLI flag takes additive precedence: final = config + CLI.
    let config_transpose_f64 = config
        .get_path("settings.transpose")
        .as_f64()
        .unwrap_or(0.0);
    let config_transpose =
        if config_transpose_f64 < f64::from(i8::MIN) || config_transpose_f64 > f64::from(i8::MAX) {
            let clamped = config_transpose_f64.clamp(f64::from(i8::MIN), f64::from(i8::MAX)) as i8;
            eprintln!(
                "warning: settings.transpose value {} is out of i8 range, clamped to {}",
                config_transpose_f64, clamped
            );
            clamped
        } else {
            config_transpose_f64 as i8
        };
    let (effective_transpose, saturated) =
        chordsketch_core::transpose::combine_transpose(config_transpose, cli.transpose);
    if saturated {
        eprintln!(
            "warning: transpose offset {} + {} exceeds i8 range, clamped to {}",
            config_transpose, cli.transpose, effective_transpose
        );
    }

    let mut all_songs: Vec<chordsketch_core::ast::Song> = Vec::new();
    let mut had_error = false;

    for path in &cli.files {
        let input = match read_file_clamped(path) {
            Ok(content) => content,
            Err(e) => {
                eprintln!("error: {path}: {e}");
                had_error = true;
                continue;
            }
        };

        let result = chordsketch_core::parse_multi_lenient(&input);
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

    // Apply selector filtering if an instrument or user is configured.
    let selector_ctx = chordsketch_core::selector::SelectorContext::from_config(&config);
    if selector_ctx.instrument.is_some() || selector_ctx.user.is_some() {
        all_songs = all_songs
            .iter()
            .map(|song| selector_ctx.filter_song(song))
            .collect();
    }

    /// Output produced by a renderer.
    enum Output {
        Text(String),
        Binary(Vec<u8>),
    }

    let output = match cli.format {
        Format::Pdf => {
            let result = chordsketch_render_pdf::render_songs_with_warnings(
                &all_songs,
                effective_transpose,
                &config,
            );
            for w in &result.warnings {
                eprintln!("warning: {w}");
            }
            Output::Binary(result.output)
        }
        Format::Text => {
            let result = chordsketch_render_text::render_songs_with_warnings(
                &all_songs,
                effective_transpose,
                &config,
            );
            for w in &result.warnings {
                eprintln!("warning: {w}");
            }
            Output::Text(result.output)
        }
        Format::Html => {
            let result = chordsketch_render_html::render_songs_with_warnings(
                &all_songs,
                effective_transpose,
                &config,
            );
            for w in &result.warnings {
                eprintln!("warning: {w}");
            }
            Output::Text(result.output)
        }
    };

    let is_empty = match &output {
        Output::Text(s) => s.is_empty(),
        Output::Binary(b) => b.is_empty(),
    };
    if had_error && is_empty {
        return ExitCode::FAILURE;
    }

    let write_result = match &output {
        Output::Binary(bytes) => write_bytes(&cli.output, bytes),
        Output::Text(text) => write_text(&cli.output, text),
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

/// Run the `fmt` subcommand with pre-parsed arguments.
///
/// # Exit codes
///
/// * `0` — all files are formatted (or `--check` found nothing to change).
/// * `1` — at least one file needs formatting (`--check` mode) or an I/O
///   error occurred.
fn run_fmt(files: &[String], check: bool) -> ExitCode {
    let options = chordsketch_core::formatter::FormatOptions::default();
    let mut had_error = false;
    let mut needs_format = false;

    for file in files {
        if file == "-" {
            // stdin → stdout (always, regardless of --check).
            let input = match read_stdin_clamped() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: reading stdin: {e}");
                    had_error = true;
                    continue;
                }
            };
            let formatted = chordsketch_core::formatter::format(&input, &options);
            if check {
                if formatted != input {
                    eprintln!("error: <stdin> is not formatted");
                    needs_format = true;
                }
            } else if let Err(e) = write_text(&None, &formatted) {
                eprintln!("error: writing stdout: {e}");
                had_error = true;
            }
        } else {
            let input = match read_file_clamped(file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: {file}: {e}");
                    had_error = true;
                    continue;
                }
            };
            let formatted = chordsketch_core::formatter::format(&input, &options);
            if check {
                if formatted != input {
                    eprintln!("error: {file}: not formatted");
                    needs_format = true;
                }
            } else if formatted != input {
                if let Err(e) = write_formatted_atomic(file, &formatted) {
                    eprintln!("error: {file}: {e}");
                    had_error = true;
                }
            }
        }
    }

    if had_error || needs_format {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Run the `convert` subcommand.
///
/// For import (no `--to` flag): reads each file, auto-detects or
/// force-converts to ChordPro format, then writes the output to
/// `output_path` (or stdout). Multiple files are concatenated with
/// `{new_song}` separators.
///
/// For export (`--to musicxml`): reads ChordPro files and converts them
/// to MusicXML 4.0. Only a single input file is supported for export.
///
/// # Exit codes
///
/// * `0` — all files converted successfully.
/// * `1` — at least one I/O error occurred or a file was skipped because its
///   format could not be detected.
fn run_convert(
    files: &[String],
    from: ConvertFrom,
    to: Option<ConvertTo>,
    output_path: Option<&str>,
) -> ExitCode {
    use chordsketch_convert_musicxml::{from_musicxml, to_musicxml};
    use chordsketch_core::{
        InputFormat, convert_abc, convert_plain_text, detect_format, parse_lenient,
        song_to_chordpro,
    };

    // --- Export path: ChordPro → MusicXML -----------------------------------
    if let Some(ConvertTo::Musicxml) = to {
        if files.len() > 1 {
            eprintln!("error: --to musicxml supports only a single input file");
            return ExitCode::FAILURE;
        }
        let file = &files[0];
        let input = if file == "-" {
            match read_stdin_clamped() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: reading stdin: {e}");
                    return ExitCode::FAILURE;
                }
            }
        } else {
            match read_file_clamped(file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: {file}: {e}");
                    return ExitCode::FAILURE;
                }
            }
        };
        let result = parse_lenient(&input);
        let xml = to_musicxml(&result.song);
        if let Err(e) = write_text(&output_path.map(str::to_string), &xml) {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
        return ExitCode::SUCCESS;
    }

    // --- Import path: other formats → ChordPro ------------------------------
    let mut had_error = false;
    let mut combined = String::new();

    for file in files {
        let input = if file == "-" {
            match read_stdin_clamped() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: reading stdin: {e}");
                    had_error = true;
                    continue;
                }
            }
        } else {
            match read_file_clamped(file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: {file}: {e}");
                    had_error = true;
                    continue;
                }
            }
        };

        // Determine which conversion to apply.
        enum Action {
            Plaintext,
            Abc,
            MusicXml,
            Passthrough,
            Skip,
        }

        let action = match from {
            ConvertFrom::Plaintext => Action::Plaintext,
            ConvertFrom::Abc => Action::Abc,
            ConvertFrom::Musicxml => Action::MusicXml,
            ConvertFrom::Auto => {
                // Check file extension first
                let ext = Path::new(file.as_str())
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_ascii_lowercase);
                if ext.as_deref() == Some("xml") || ext.as_deref() == Some("musicxml") {
                    Action::MusicXml
                } else {
                    let fmt = detect_format(&input);
                    match fmt {
                        InputFormat::PlainChordLyrics => Action::Plaintext,
                        InputFormat::Abc => Action::Abc,
                        InputFormat::ChordPro => Action::Passthrough,
                        InputFormat::Unknown => Action::Skip,
                    }
                }
            }
        };

        match action {
            Action::Plaintext => {
                if !combined.is_empty() {
                    combined.push_str("{new_song}\n");
                }
                let song = convert_plain_text(&input);
                combined.push_str(&song_to_chordpro(&song));
            }
            Action::Abc => {
                if !combined.is_empty() {
                    combined.push_str("{new_song}\n");
                }
                combined.push_str(&convert_abc(&input));
            }
            Action::MusicXml => {
                if !combined.is_empty() {
                    combined.push_str("{new_song}\n");
                }
                match from_musicxml(&input) {
                    Ok(song) => combined.push_str(&song_to_chordpro(&song)),
                    Err(e) => {
                        let label = if file == "-" {
                            "<stdin>"
                        } else {
                            file.as_str()
                        };
                        eprintln!("error: {label}: {e}");
                        had_error = true;
                    }
                }
            }
            Action::Passthrough => {
                // Already ChordPro — pass through unchanged.
                if !combined.is_empty() {
                    combined.push_str("{new_song}\n");
                }
                combined.push_str(&input);
            }
            Action::Skip => {
                let label = if file == "-" {
                    "<stdin>"
                } else {
                    file.as_str()
                };
                eprintln!(
                    "warning: {label}: format could not be detected; \
                     use --from plaintext, --from abc, or --from musicxml to force conversion"
                );
                had_error = true;
            }
        }
    }

    if combined.is_empty() && had_error {
        return ExitCode::FAILURE;
    }

    if let Err(e) = write_text(&output_path.map(str::to_string), &combined) {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }

    if had_error {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Write `content` to `path` atomically: write to a temp file in the same
/// directory, then rename over `path`. On POSIX, `rename(2)` is atomic on the
/// same filesystem, so the original file is never left partially written.
///
/// File permissions are preserved without a TOCTOU window: the original file's
/// mode is applied to the temp file *before* `persist()` (rename). Because
/// `rename(2)` transfers the source inode, the resulting file already has the
/// correct permissions the instant it becomes visible at `path`.
fn write_formatted_atomic(path: &str, content: &str) -> io::Result<()> {
    let parent = Path::new(path).parent().unwrap_or(Path::new("."));
    // Capture original permissions before any write.
    let orig_perms = fs::metadata(path).ok().map(|m| m.permissions());
    let mut tmp = NamedTempFile::new_in(parent)?;
    tmp.write_all(content.as_bytes())?;
    // Apply the original file's mode to the temp file *before* the rename so
    // there is no window where the file is visible on disk with 0o600.
    if let Some(ref perms) = orig_perms {
        tmp.as_file().set_permissions(perms.clone())?;
    }
    tmp.persist(path).map_err(io::Error::other)?;
    Ok(())
}
