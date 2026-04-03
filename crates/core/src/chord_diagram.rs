//! SVG chord diagram generator.
//!
//! Generates inline SVG chord diagram strings from chord definition data.
//! The diagrams show fret positions, open/muted strings, and finger numbers
//! for fretted string instruments.
//!
//! # Examples
//!
//! ```
//! use chordpro_core::chord_diagram::{DiagramData, render_svg};
//!
//! let data = DiagramData {
//!     name: "Am".to_string(),
//!     display_name: None,
//!     strings: 6,
//!     frets_shown: 5,
//!     base_fret: 1,
//!     frets: vec![-1, 0, 2, 2, 1, 0],
//!     fingers: vec![],
//! };
//! let svg = render_svg(&data);
//! assert!(svg.contains("<svg"));
//! assert!(svg.contains("Am"));
//! ```

/// Minimum number of strings for a valid chord diagram.
pub const MIN_STRINGS: usize = 2;

/// Maximum number of strings for a valid chord diagram (covers 12-string guitar).
pub const MAX_STRINGS: usize = 12;

/// Default number of frets shown in a chord diagram.
pub const DEFAULT_FRETS_SHOWN: usize = 5;

/// Maximum allowed `base_fret` value (typical 24-fret guitar).
pub const MAX_BASE_FRET: u32 = 24;

/// Minimum number of frets shown in a rendered chord diagram.
pub const MIN_FRETS_SHOWN: usize = 1;

/// Maximum number of frets shown in a rendered chord diagram (24-fret instrument).
pub const MAX_FRETS_SHOWN: usize = 24;

/// Data needed to render a chord diagram.
#[derive(Debug, Clone)]
pub struct DiagramData {
    /// Chord name (identifier used for matching).
    pub name: String,
    /// Display name override from `{define}` `display` attribute.
    ///
    /// When present, diagram titles should show this instead of `name`.
    pub display_name: Option<String>,
    /// Number of strings (e.g., 6 for guitar, 4 for ukulele).
    ///
    /// Valid range: 2–12 (enforced by [`from_raw`](Self::from_raw)).
    pub strings: usize,
    /// Number of frets shown in the diagram.
    pub frets_shown: usize,
    /// The base fret (1 = open position, >1 shows fret number).
    pub base_fret: u32,
    /// Fret values for each string: -1 = muted (x), 0 = open, 1+ = fret number.
    ///
    /// Positive values are clamped to `frets_shown` to prevent rendering
    /// outside the visible grid.
    pub frets: Vec<i32>,
    /// Optional finger numbers for each string (0 = none).
    pub fingers: Vec<u8>,
}

impl DiagramData {
    /// Returns the title to display above the diagram.
    ///
    /// Uses the `display_name` override if present, otherwise falls back to `name`.
    #[must_use]
    pub fn title(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.name)
    }
}

impl DiagramData {
    /// Parse fretted chord data from a raw definition string, inferring the
    /// string count from the number of fret values provided.
    ///
    /// This is equivalent to calling [`from_raw`](Self::from_raw) with
    /// `num_strings` set to `0`, so the actual fret count determines the
    /// number of strings in the diagram.
    #[must_use]
    pub fn from_raw_infer(name: &str, raw: &str) -> Option<Self> {
        Self::from_raw(name, raw, 0)
    }

    /// Like [`from_raw_infer`](Self::from_raw_infer) but uses a custom
    /// `frets_shown` value instead of the default (5).
    #[must_use]
    pub fn from_raw_infer_frets(name: &str, raw: &str, frets_shown: usize) -> Option<Self> {
        Self::from_raw_frets(name, raw, 0, frets_shown)
    }

    /// Parse fretted chord data from a `ChordDefinition` raw string.
    ///
    /// Expected format: `base-fret N frets f1 f2 ... [fingers g1 g2 ...]`
    /// where fret values are numbers or `x`/`X` for muted strings.
    ///
    /// `num_strings` sets a minimum string count; the actual count will be
    /// the maximum of this value and the number of fret values parsed.
    ///
    /// Returns `None` if:
    /// - No fret values are provided
    /// - The resulting string count is outside the valid range (2–12)
    ///
    /// Positive fret values exceeding `frets_shown` are clamped to
    /// prevent rendering outside the visible grid. Uses the default
    /// frets shown value ([`DEFAULT_FRETS_SHOWN`]).
    #[must_use]
    pub fn from_raw(name: &str, raw: &str, num_strings: usize) -> Option<Self> {
        Self::from_raw_frets(name, raw, num_strings, DEFAULT_FRETS_SHOWN)
    }

    /// Like [`from_raw`](Self::from_raw) but uses a custom `frets_shown`
    /// value instead of the default.
    ///
    /// The keywords `display` and `format` act as stop-words that terminate
    /// fret and finger value parsing when encountered as tokens.
    #[must_use]
    pub fn from_raw_frets(
        name: &str,
        raw: &str,
        num_strings: usize,
        frets_shown: usize,
    ) -> Option<Self> {
        let mut base_fret: u32 = 1;
        let mut frets: Vec<i32> = Vec::new();
        let mut fingers: Vec<u8> = Vec::new();

        let tokens: Vec<&str> = raw.split_whitespace().collect();
        let mut i = 0;
        while i < tokens.len() {
            let tok_lower = tokens[i].to_ascii_lowercase();
            match tok_lower.as_str() {
                "base-fret" => {
                    if i + 1 < tokens.len() {
                        base_fret = tokens[i + 1].parse().unwrap_or(1).clamp(1, MAX_BASE_FRET);
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                "frets" => {
                    i += 1;
                    while i < tokens.len() {
                        let low = tokens[i].to_ascii_lowercase();
                        if matches!(
                            low.as_str(),
                            "frets" | "fingers" | "base-fret" | "display" | "format"
                        ) {
                            break;
                        }
                        let val = match low.as_str() {
                            "x" | "n" => -1,
                            s => {
                                let v = s.parse::<i32>().unwrap_or(-1);
                                if v < -1 { -1 } else { v }
                            }
                        };
                        frets.push(val);
                        i += 1;
                    }
                }
                "fingers" => {
                    i += 1;
                    while i < tokens.len() {
                        let low = tokens[i].to_ascii_lowercase();
                        if matches!(
                            low.as_str(),
                            "frets" | "fingers" | "base-fret" | "display" | "format"
                        ) {
                            break;
                        }
                        fingers.push(tokens[i].parse().unwrap_or(0));
                        i += 1;
                    }
                }
                _ => {
                    i += 1;
                }
            }
        }

        if frets.is_empty() {
            return None;
        }

        let strings = num_strings.max(frets.len());

        // Validate string count is within reasonable range.
        if !(MIN_STRINGS..=MAX_STRINGS).contains(&strings) {
            return None;
        }

        // Clamp frets_shown to the valid range, mirroring base_fret clamping.
        let frets_shown = frets_shown.clamp(MIN_FRETS_SHOWN, MAX_FRETS_SHOWN);

        // Clamp positive fret values to the visible range to prevent
        // rendering dots outside the fret grid.
        let frets: Vec<i32> = frets
            .into_iter()
            .map(|f| {
                if f > frets_shown as i32 {
                    frets_shown as i32
                } else {
                    f
                }
            })
            .collect();

        Some(Self {
            name: name.to_string(),
            display_name: None,
            strings,
            frets_shown,
            base_fret,
            frets,
            fingers,
        })
    }
}

// ---------------------------------------------------------------------------
// SVG rendering constants
// ---------------------------------------------------------------------------

/// Cell width in SVG user units (pixels at 1:1 zoom).
///
/// Intentionally larger than the PDF renderer's 10.0 pt because SVG
/// diagrams target on-screen display where more generous spacing
/// improves readability. The PDF renderer uses smaller values (10x12 pt)
/// suited for printed page layout.
const CELL_W: f32 = 16.0;
/// Cell height in SVG user units. See [`CELL_W`] for rationale.
const CELL_H: f32 = 20.0;
const TOP_MARGIN: f32 = 30.0;
const LEFT_MARGIN: f32 = 20.0;
const DOT_RADIUS: f32 = 5.0;
const OPEN_RADIUS: f32 = 4.0;

/// Render a chord diagram as an inline SVG string.
#[must_use]
pub fn render_svg(data: &DiagramData) -> String {
    if data.strings < MIN_STRINGS
        || data.strings > MAX_STRINGS
        || data.frets_shown < MIN_FRETS_SHOWN
        || data.frets_shown > MAX_FRETS_SHOWN
    {
        return String::new();
    }
    let num_strings = data.strings;
    let num_frets = data.frets_shown;
    let grid_w = (num_strings - 1) as f32 * CELL_W;
    let grid_h = num_frets as f32 * CELL_H;
    let total_w = grid_w + LEFT_MARGIN * 2.0;
    let total_h = grid_h + TOP_MARGIN + 30.0;

    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{total_w}\" height=\"{total_h}\" \
         viewBox=\"0 0 {total_w} {total_h}\" class=\"chord-diagram\">\n"
    );

    // Chord name (uses display override if present)
    let name_x = LEFT_MARGIN + grid_w / 2.0;
    svg.push_str(&format!(
        "<text x=\"{name_x}\" y=\"15\" text-anchor=\"middle\" \
         font-family=\"sans-serif\" font-size=\"14\" font-weight=\"bold\">{}</text>\n",
        crate::escape::escape_xml(data.title())
    ));

    // Nut or base-fret indicator
    let nut_y = TOP_MARGIN;
    if data.base_fret == 1 {
        svg.push_str(&format!(
            "<line x1=\"{LEFT_MARGIN}\" y1=\"{nut_y}\" x2=\"{}\" y2=\"{nut_y}\" \
             stroke=\"black\" stroke-width=\"3\"/>\n",
            LEFT_MARGIN + grid_w
        ));
    } else {
        svg.push_str(&format!(
            "<text x=\"{}\" y=\"{}\" text-anchor=\"end\" \
             font-family=\"sans-serif\" font-size=\"10\">{}fr</text>\n",
            LEFT_MARGIN - 4.0,
            nut_y + CELL_H / 2.0 + 3.0,
            data.base_fret
        ));
    }

    // Vertical lines (strings)
    for i in 0..num_strings {
        let x = LEFT_MARGIN + i as f32 * CELL_W;
        svg.push_str(&format!(
            "<line x1=\"{x}\" y1=\"{nut_y}\" x2=\"{x}\" y2=\"{}\" \
             stroke=\"black\" stroke-width=\"1\"/>\n",
            nut_y + grid_h
        ));
    }

    // Horizontal lines (frets)
    for j in 0..=num_frets {
        let y = nut_y + j as f32 * CELL_H;
        svg.push_str(&format!(
            "<line x1=\"{LEFT_MARGIN}\" y1=\"{y}\" x2=\"{}\" y2=\"{y}\" \
             stroke=\"black\" stroke-width=\"1\"/>\n",
            LEFT_MARGIN + grid_w
        ));
    }

    // Finger positions, open, and muted markers
    for (i, &fret) in data.frets.iter().enumerate() {
        if i >= num_strings {
            break;
        }
        let x = LEFT_MARGIN + i as f32 * CELL_W;
        if fret == -1 {
            // Muted (X)
            let y = nut_y - 10.0;
            svg.push_str(&format!(
                "<text x=\"{x}\" y=\"{y}\" text-anchor=\"middle\" \
                 font-family=\"sans-serif\" font-size=\"10\">X</text>\n"
            ));
        } else if fret == 0 {
            // Open (O)
            let y = nut_y - 10.0;
            svg.push_str(&format!(
                "<circle cx=\"{x}\" cy=\"{y}\" r=\"{OPEN_RADIUS}\" \
                 fill=\"none\" stroke=\"black\" stroke-width=\"1\"/>\n"
            ));
        } else {
            // Fretted dot
            let y = nut_y + (fret as f32 - 0.5) * CELL_H;
            svg.push_str(&format!(
                "<circle cx=\"{x}\" cy=\"{y}\" r=\"{DOT_RADIUS}\" fill=\"black\"/>\n"
            ));
            // Finger number inside the dot (if available and non-zero)
            if let Some(&finger) = data.fingers.get(i) {
                if finger > 0 {
                    svg.push_str(&format!(
                        "<text x=\"{x}\" y=\"{}\" text-anchor=\"middle\" \
                         font-family=\"sans-serif\" font-size=\"8\" \
                         fill=\"white\">{finger}</text>\n",
                        y + 3.0
                    ));
                }
            }
        }
    }

    svg.push_str("</svg>");
    svg
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_svg_basic() {
        let data = DiagramData {
            name: "Am".to_string(),
            display_name: None,
            strings: 6,
            frets_shown: 5,
            base_fret: 1,
            frets: vec![-1, 0, 2, 2, 1, 0],
            fingers: vec![],
        };
        let svg = render_svg(&data);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
        assert!(svg.contains("Am"));
        // Should have circles for fretted positions
        assert!(svg.contains("<circle"));
        // Should have X for muted string
        assert!(svg.contains(">X<"));
    }

    #[test]
    fn test_render_svg_barre_chord() {
        let data = DiagramData {
            name: "F".to_string(),
            display_name: None,
            strings: 6,
            frets_shown: 5,
            base_fret: 1,
            frets: vec![1, 1, 2, 3, 3, 1],
            fingers: vec![],
        };
        let svg = render_svg(&data);
        assert!(svg.contains(">F<"));
    }

    #[test]
    fn test_render_svg_high_position() {
        let data = DiagramData {
            name: "Bm".to_string(),
            display_name: None,
            strings: 6,
            frets_shown: 5,
            base_fret: 7,
            frets: vec![-1, 1, 3, 3, 2, 1],
            fingers: vec![],
        };
        let svg = render_svg(&data);
        assert!(svg.contains("7fr"));
    }

    #[test]
    fn test_from_raw_basic() {
        let data = DiagramData::from_raw("Am", "base-fret 1 frets x 0 2 2 1 0", 6).unwrap();
        assert_eq!(data.name, "Am");
        assert_eq!(data.base_fret, 1);
        assert_eq!(data.frets, vec![-1, 0, 2, 2, 1, 0]);
    }

    #[test]
    fn test_from_raw_with_fingers() {
        let data =
            DiagramData::from_raw("C", "base-fret 1 frets x 3 2 0 1 0 fingers 0 3 2 0 1 0", 6)
                .unwrap();
        assert_eq!(data.frets, vec![-1, 3, 2, 0, 1, 0]);
        assert_eq!(data.fingers, vec![0, 3, 2, 0, 1, 0]);
    }

    #[test]
    fn test_from_raw_no_frets() {
        assert!(DiagramData::from_raw("X", "base-fret 1", 6).is_none());
    }

    #[test]
    fn test_from_raw_ukulele() {
        let data = DiagramData::from_raw("C", "frets 0 0 0 3", 4).unwrap();
        assert_eq!(data.strings, 4);
        assert_eq!(data.frets, vec![0, 0, 0, 3]);
    }

    #[test]
    fn test_from_raw_infer_guitar() {
        let data = DiagramData::from_raw_infer("Am", "base-fret 1 frets x 0 2 2 1 0").unwrap();
        assert_eq!(data.strings, 6);
        assert_eq!(data.frets, vec![-1, 0, 2, 2, 1, 0]);
    }

    #[test]
    fn test_from_raw_infer_ukulele() {
        let data = DiagramData::from_raw_infer("C", "frets 0 0 0 3").unwrap();
        assert_eq!(data.strings, 4);
        assert_eq!(data.frets, vec![0, 0, 0, 3]);
    }

    #[test]
    fn test_from_raw_infer_banjo() {
        let data = DiagramData::from_raw_infer("G", "frets 0 0 0 0 0").unwrap();
        assert_eq!(data.strings, 5);
        assert_eq!(data.frets, vec![0, 0, 0, 0, 0]);
    }

    #[test]
    fn test_title_returns_display_name_when_set() {
        let data = DiagramData {
            name: "Am".to_string(),
            display_name: Some("A minor".to_string()),
            strings: 6,
            frets_shown: 5,
            base_fret: 1,
            frets: vec![-1, 0, 2, 2, 1, 0],
            fingers: vec![],
        };
        assert_eq!(data.title(), "A minor");
    }

    #[test]
    fn test_title_falls_back_to_name() {
        let data = DiagramData {
            name: "Am".to_string(),
            display_name: None,
            strings: 6,
            frets_shown: 5,
            base_fret: 1,
            frets: vec![-1, 0, 2, 2, 1, 0],
            fingers: vec![],
        };
        assert_eq!(data.title(), "Am");
    }

    #[test]
    fn test_render_svg_with_display_name() {
        let data = DiagramData {
            name: "Am".to_string(),
            display_name: Some("A minor".to_string()),
            strings: 6,
            frets_shown: 5,
            base_fret: 1,
            frets: vec![-1, 0, 2, 2, 1, 0],
            fingers: vec![],
        };
        let svg = render_svg(&data);
        assert!(svg.contains("A minor"));
        assert!(!svg.contains(">Am<"));
    }

    #[test]
    fn test_from_raw_display_name_is_none() {
        let data = DiagramData::from_raw_infer("Am", "base-fret 1 frets x 0 2 2 1 0").unwrap();
        assert!(data.display_name.is_none());
    }

    // --- Finger number rendering (#473) ---

    #[test]
    fn test_render_svg_with_finger_numbers() {
        let data = DiagramData {
            name: "C".to_string(),
            display_name: None,
            strings: 6,
            frets_shown: 5,
            base_fret: 1,
            frets: vec![-1, 3, 2, 0, 1, 0],
            fingers: vec![0, 3, 2, 0, 1, 0],
        };
        let svg = render_svg(&data);
        // Fingers 3, 2, 1 should appear as white text inside dots
        assert!(svg.contains("fill=\"white\">3</text>"));
        assert!(svg.contains("fill=\"white\">2</text>"));
        assert!(svg.contains("fill=\"white\">1</text>"));
    }

    #[test]
    fn test_render_svg_zero_finger_not_shown() {
        let data = DiagramData {
            name: "Am".to_string(),
            display_name: None,
            strings: 6,
            frets_shown: 5,
            base_fret: 1,
            frets: vec![-1, 0, 2, 2, 1, 0],
            fingers: vec![0, 0, 2, 3, 1, 0],
        };
        let svg = render_svg(&data);
        // Finger 0 should NOT appear
        assert!(!svg.contains("fill=\"white\">0</text>"));
        // Non-zero fingers should appear
        assert!(svg.contains("fill=\"white\">2</text>"));
        assert!(svg.contains("fill=\"white\">3</text>"));
        assert!(svg.contains("fill=\"white\">1</text>"));
    }

    #[test]
    fn test_render_svg_no_fingers_no_crash() {
        let data = DiagramData {
            name: "G".to_string(),
            display_name: None,
            strings: 6,
            frets_shown: 5,
            base_fret: 1,
            frets: vec![3, 2, 0, 0, 0, 3],
            fingers: vec![],
        };
        let svg = render_svg(&data);
        assert!(svg.contains("<svg"));
        assert!(!svg.contains("fill=\"white\""));
    }

    #[test]
    fn test_render_svg_fewer_fingers_than_frets() {
        let data = DiagramData {
            name: "Am".to_string(),
            display_name: None,
            strings: 6,
            frets_shown: 5,
            base_fret: 1,
            frets: vec![-1, 0, 2, 2, 1, 0],
            fingers: vec![0, 0, 2],
        };
        let svg = render_svg(&data);
        // Should only render finger 2, no crash for missing fingers
        assert!(svg.contains("fill=\"white\">2</text>"));
        assert!(svg.contains("<svg"));
    }

    // --- num_strings validation (#467) ---

    #[test]
    fn test_from_raw_zero_strings_rejected() {
        // Single fret value -> 1 string, below MIN_STRINGS
        assert!(DiagramData::from_raw("X", "frets 0", 0).is_none());
    }

    #[test]
    fn test_from_raw_one_string_rejected() {
        assert!(DiagramData::from_raw("X", "frets 0", 1).is_none());
    }

    #[test]
    fn test_from_raw_two_strings_accepted() {
        let data = DiagramData::from_raw("X", "frets 0 0", 0).unwrap();
        assert_eq!(data.strings, 2);
    }

    #[test]
    fn test_from_raw_twelve_strings_accepted() {
        let data = DiagramData::from_raw("X", "frets 0 0 0 0 0 0 0 0 0 0 0 0", 0).unwrap();
        assert_eq!(data.strings, 12);
    }

    #[test]
    fn test_from_raw_thirteen_strings_rejected() {
        assert!(DiagramData::from_raw("X", "frets 0 0 0 0 0 0 0 0 0 0 0 0 0", 0,).is_none());
    }

    #[test]
    fn test_from_raw_num_strings_forces_too_many() {
        // 6 frets but num_strings=13 -> rejected
        assert!(DiagramData::from_raw("X", "frets 0 0 0 0 0 0", 13).is_none());
    }

    // --- Fret value clamping (#469) ---

    #[test]
    fn test_fret_exceeding_range_clamped() {
        let data = DiagramData::from_raw("X", "base-fret 1 frets 0 12 0 0 0 0", 6).unwrap();
        // Fret 12 should be clamped to frets_shown (5)
        assert_eq!(data.frets[1], 5);
    }

    #[test]
    fn test_fret_at_boundary_not_clamped() {
        let data = DiagramData::from_raw("X", "base-fret 1 frets 0 5 0 0 0 0", 6).unwrap();
        assert_eq!(data.frets[1], 5);
    }

    #[test]
    fn test_fret_within_range_unchanged() {
        let data = DiagramData::from_raw("X", "base-fret 1 frets 0 3 0 0 0 0", 6).unwrap();
        assert_eq!(data.frets[1], 3);
    }

    #[test]
    fn test_muted_and_open_not_clamped() {
        let data = DiagramData::from_raw("X", "frets x 0 1 2 3 4", 6).unwrap();
        assert_eq!(data.frets[0], -1); // muted
        assert_eq!(data.frets[1], 0); // open
    }

    #[test]
    fn test_extreme_fret_value_clamped() {
        let data = DiagramData::from_raw("X", "frets 1000 0 0 0 0 0", 6).unwrap();
        assert_eq!(data.frets[0], 5);
    }

    // --- Edge case tests (#472) ---

    #[test]
    fn test_seven_string_instrument() {
        let data = DiagramData::from_raw("X", "frets 0 0 0 0 0 0 0", 7).unwrap();
        assert_eq!(data.strings, 7);
        let svg = render_svg(&data);
        assert!(svg.contains("<svg"));
    }

    #[test]
    fn test_eight_string_instrument() {
        let data = DiagramData::from_raw("X", "frets 0 0 0 0 0 0 0 0", 8).unwrap();
        assert_eq!(data.strings, 8);
        let svg = render_svg(&data);
        assert!(svg.contains("<svg"));
    }

    #[test]
    fn test_twelve_string_renders_without_panic() {
        let data = DiagramData::from_raw("G12", "frets 0 0 0 0 0 0 0 0 0 0 0 0", 12).unwrap();
        let svg = render_svg(&data);
        assert!(svg.contains("G12"));
    }

    #[test]
    fn test_fewer_fingers_than_frets() {
        let data = DiagramData::from_raw("Am", "frets x 0 2 2 1 0 fingers 0 0 2", 6).unwrap();
        assert_eq!(data.frets.len(), 6);
        assert_eq!(data.fingers.len(), 3);
        // Should render without panic
        let svg = render_svg(&data);
        assert!(svg.contains("<svg"));
    }

    #[test]
    fn test_empty_frets_rejected() {
        assert!(DiagramData::from_raw("X", "base-fret 1", 6).is_none());
    }

    #[test]
    fn test_non_numeric_fret_treated_as_muted() {
        let data = DiagramData::from_raw("X", "frets abc 0 0 0 0 0", 6).unwrap();
        // Non-numeric values parsed as -1 (same as 'x')
        assert_eq!(data.frets[0], -1);
    }

    #[test]
    fn test_extreme_base_fret() {
        // base_fret is clamped to 1..=24
        let data = DiagramData::from_raw("X", "base-fret 1000 frets 1 2 3 4 5 6", 6).unwrap();
        assert_eq!(data.base_fret, 24);
        let svg = render_svg(&data);
        assert!(svg.contains("24fr"));
    }

    #[test]
    fn test_all_muted_strings() {
        let data = DiagramData::from_raw("X", "frets x x x x x x", 6).unwrap();
        assert!(data.frets.iter().all(|&f| f == -1));
        let svg = render_svg(&data);
        assert!(svg.contains("<svg"));
    }

    #[test]
    fn test_all_open_strings() {
        let data = DiagramData::from_raw("Open", "frets 0 0 0 0 0 0", 6).unwrap();
        assert!(data.frets.iter().all(|&f| f == 0));
        let svg = render_svg(&data);
        assert!(svg.contains("<svg"));
    }

    #[test]
    fn test_format_stops_fret_parsing() {
        // "format" as a standalone token should act as a stop-word,
        // preventing it from being misinterpreted as a fret value.
        let data = DiagramData::from_raw("Am", "base-fret 1 frets x 0 2 2 1 0 format", 6).unwrap();
        assert_eq!(data.frets, vec![-1, 0, 2, 2, 1, 0]);
    }

    // --- base-fret stop-word and clamping (#603, #604) ---

    #[test]
    fn test_base_fret_after_frets_is_stop_word() {
        // "base-fret" appearing after "frets" should stop fret parsing,
        // not be treated as a fret value.
        let data = DiagramData::from_raw("Am", "frets x 0 2 2 1 0 base-fret 3", 6).unwrap();
        assert_eq!(data.frets, vec![-1, 0, 2, 2, 1, 0]);
        assert_eq!(data.base_fret, 3);
    }

    #[test]
    fn test_base_fret_after_fingers_is_stop_word() {
        // "base-fret" should also stop finger parsing.
        let data =
            DiagramData::from_raw("C", "frets x 3 2 0 1 0 fingers 0 3 2 0 1 0 base-fret 2", 6)
                .unwrap();
        assert_eq!(data.fingers, vec![0, 3, 2, 0, 1, 0]);
        assert_eq!(data.base_fret, 2);
    }

    #[test]
    fn test_format_stops_finger_parsing() {
        // "format" should stop finger parsing.
        let data =
            DiagramData::from_raw("C", "frets x 3 2 0 1 0 fingers 0 3 2 0 1 0 format", 6).unwrap();
        assert_eq!(data.fingers, vec![0, 3, 2, 0, 1, 0]);
    }

    #[test]
    fn test_base_fret_zero_clamped_to_one() {
        let data = DiagramData::from_raw("Am", "base-fret 0 frets x 0 2 2 1 0", 6).unwrap();
        assert_eq!(data.base_fret, 1);
    }

    #[test]
    fn test_base_fret_negative_defaults_to_one() {
        // Negative value fails u32 parse, falls back to unwrap_or(1).
        let data = DiagramData::from_raw("Am", "base-fret -1 frets x 0 2 2 1 0", 6).unwrap();
        assert_eq!(data.base_fret, 1);
    }

    #[test]
    fn test_base_fret_large_value_clamped() {
        let data = DiagramData::from_raw("Am", "base-fret 100 frets x 0 2 2 1 0", 6).unwrap();
        assert_eq!(data.base_fret, 24);
    }

    // --- render_svg input guard (#606) ---

    #[test]
    fn test_render_svg_zero_strings_returns_empty() {
        let data = DiagramData {
            name: "X".to_string(),
            display_name: None,
            strings: 0,
            frets_shown: 5,
            base_fret: 1,
            frets: vec![],
            fingers: vec![],
        };
        assert!(render_svg(&data).is_empty());
    }

    #[test]
    fn test_render_svg_one_string_returns_empty() {
        let data = DiagramData {
            name: "X".to_string(),
            display_name: None,
            strings: 1,
            frets_shown: 5,
            base_fret: 1,
            frets: vec![0],
            fingers: vec![],
        };
        assert!(render_svg(&data).is_empty());
    }

    // --- "frets" stop-word in fingers parser (#616) ---

    #[test]
    fn test_frets_stops_finger_parsing() {
        // "frets" after "fingers" should stop finger parsing. The outer
        // loop then re-enters the "frets" arm for the second batch, so
        // both batches of fret values are accumulated.
        let data =
            DiagramData::from_raw("Am", "frets x 0 2 2 1 0 fingers 0 0 2 frets x 0 2 2 1 0", 6)
                .unwrap();
        // Only 3 finger values before the second "frets" token.
        assert_eq!(data.fingers, vec![0, 0, 2]);
        // Both frets batches (6 + 6) are accumulated by the outer loop.
        assert_eq!(data.frets.len(), 12);
        // strings is max(num_strings, frets.len()) = max(6, 12) = 12.
        assert_eq!(data.strings, 12);
    }

    #[test]
    fn test_repeated_frets_keyword_not_consumed_as_value() {
        // The "frets" keyword should not be consumed as a fret value.
        // Without the stop-word, "frets" would parse as -1 via unwrap_or.
        let data = DiagramData::from_raw("Am", "frets 1 2 3 frets 4 5 6", 0).unwrap();
        // The outer loop processes both "frets" arms; 3 + 3 = 6 values.
        assert_eq!(data.frets.len(), 6);
        // No -1 value from "frets" being misinterpreted as a fret.
        assert!(!data.frets.contains(&-1));
    }

    // --- from_raw_frets / from_raw_infer_frets tests (#623) ---

    #[test]
    fn test_from_raw_frets_custom_frets_shown() {
        let data =
            DiagramData::from_raw_frets("Am", "base-fret 1 frets x 0 2 2 1 0", 6, 4).unwrap();
        assert_eq!(data.frets_shown, 4);
    }

    #[test]
    fn test_from_raw_infer_frets_custom_frets_shown() {
        let data =
            DiagramData::from_raw_infer_frets("Am", "base-fret 1 frets x 0 2 2 1 0", 3).unwrap();
        assert_eq!(data.frets_shown, 3);
    }

    #[test]
    fn test_from_raw_frets_clamps_fret_values() {
        // With frets_shown=3, fret value 5 should be clamped to 3.
        let data =
            DiagramData::from_raw_frets("Am", "base-fret 1 frets 0 5 0 0 0 0", 6, 3).unwrap();
        assert_eq!(data.frets[1], 3);
    }

    // --- render_svg frets_shown guard (#625) ---

    #[test]
    fn test_render_svg_zero_frets_shown_returns_empty() {
        let data = DiagramData {
            name: "X".to_string(),
            display_name: None,
            strings: 6,
            frets_shown: 0,
            base_fret: 1,
            frets: vec![0, 0, 0, 0, 0, 0],
            fingers: vec![],
        };
        assert!(render_svg(&data).is_empty());
    }

    // --- render_svg MAX_STRINGS guard (#658) ---

    #[test]
    fn test_render_svg_exceeding_max_strings_returns_empty() {
        let data = DiagramData {
            name: "X".to_string(),
            display_name: None,
            strings: 13,
            frets_shown: 5,
            base_fret: 1,
            frets: vec![0; 13],
            fingers: vec![],
        };
        assert!(render_svg(&data).is_empty());
    }

    #[test]
    fn test_render_svg_at_max_strings_ok() {
        let data = DiagramData {
            name: "X".to_string(),
            display_name: None,
            strings: MAX_STRINGS,
            frets_shown: 5,
            base_fret: 1,
            frets: vec![0; MAX_STRINGS],
            fingers: vec![],
        };
        assert!(!render_svg(&data).is_empty());
    }

    // --- render_svg MAX_FRETS_SHOWN guard (#675) ---

    #[test]
    fn test_render_svg_exceeding_max_frets_shown_returns_empty() {
        let data = DiagramData {
            name: "X".to_string(),
            display_name: None,
            strings: 6,
            frets_shown: MAX_FRETS_SHOWN + 1,
            base_fret: 1,
            frets: vec![0; 6],
            fingers: vec![],
        };
        assert!(render_svg(&data).is_empty());
    }

    #[test]
    fn test_render_svg_at_max_frets_shown_ok() {
        let data = DiagramData {
            name: "X".to_string(),
            display_name: None,
            strings: 6,
            frets_shown: MAX_FRETS_SHOWN,
            base_fret: 1,
            frets: vec![0; 6],
            fingers: vec![],
        };
        assert!(!render_svg(&data).is_empty());
    }

    // --- from_raw_frets clamps frets_shown (#691) ---

    #[test]
    fn test_from_raw_frets_clamps_excessive_frets_shown() {
        // frets_shown > MAX_FRETS_SHOWN should be clamped, not rejected.
        let data = DiagramData::from_raw_frets("Am", "frets x 0 2 2 1 0", 6, 100).unwrap();
        assert_eq!(data.frets_shown, MAX_FRETS_SHOWN);
    }

    #[test]
    fn test_from_raw_frets_clamps_zero_frets_shown() {
        // frets_shown = 0 should be clamped to MIN_FRETS_SHOWN, not rejected.
        let data = DiagramData::from_raw_frets("Am", "frets x 0 2 2 1 0", 6, 0).unwrap();
        assert_eq!(data.frets_shown, MIN_FRETS_SHOWN);
    }

    // --- "fingers" self-referencing stop-word (#647) ---

    #[test]
    fn test_duplicate_fingers_keyword_is_stop_word() {
        // Duplicate "fingers" keyword should stop finger parsing, not be
        // parsed as a finger value.
        let data = DiagramData::from_raw("Am", "frets x 0 2 2 1 0 fingers 0 0 2 fingers 0 0 2", 6)
            .unwrap();
        // First "fingers" arm collects [0, 0, 2], stops at second "fingers";
        // outer loop re-enters the arm and collects [0, 0, 2] → total 6.
        assert_eq!(data.fingers.len(), 6);
        // No spurious 0 from parsing the keyword.
        assert_eq!(data.fingers, vec![0, 0, 2, 0, 0, 2]);
    }

    #[test]
    fn test_finger_overflow_beyond_u8_max_becomes_zero() {
        // Values > 255 overflow u8::parse and fall back to 0 via unwrap_or.
        let data =
            DiagramData::from_raw("Am", "frets x 0 2 2 1 0 fingers 256 1 2 3 1 0", 6).unwrap();
        assert_eq!(data.fingers[0], 0, "256 should overflow u8 and become 0");
        assert_eq!(data.fingers[1], 1);
    }

    // --- Negative fret clamping (#648) ---

    #[test]
    fn test_negative_fret_below_minus_one_clamped() {
        let data = DiagramData::from_raw("X", "frets -5 0 2 2 1 0", 6).unwrap();
        // -5 should be treated as -1 (muted)
        assert_eq!(data.frets[0], -1);
    }

    #[test]
    fn test_minus_one_fret_unchanged() {
        let data = DiagramData::from_raw("X", "frets -1 0 2 2 1 0", 6).unwrap();
        assert_eq!(data.frets[0], -1);
    }

    #[test]
    fn test_large_negative_fret_clamped() {
        let data = DiagramData::from_raw("X", "frets -100 0 0 0 0 0", 6).unwrap();
        assert_eq!(data.frets[0], -1);
    }

    // --- Case-insensitive keyword matching (#651) ---

    #[test]
    fn test_mixed_case_keywords() {
        let data =
            DiagramData::from_raw("Am", "Base-Fret 3 Frets x 0 2 2 1 0 Fingers 0 0 2 3 1 0", 6)
                .unwrap();
        assert_eq!(data.base_fret, 3);
        assert_eq!(data.frets, vec![-1, 0, 2, 2, 1, 0]);
        assert_eq!(data.fingers, vec![0, 0, 2, 3, 1, 0]);
    }

    #[test]
    fn test_uppercase_keywords() {
        let data =
            DiagramData::from_raw("Am", "BASE-FRET 2 FRETS X 0 2 2 1 0 FINGERS 0 0 2 3 1 0", 6)
                .unwrap();
        assert_eq!(data.base_fret, 2);
        assert_eq!(data.frets, vec![-1, 0, 2, 2, 1, 0]);
        assert_eq!(data.fingers, vec![0, 0, 2, 3, 1, 0]);
    }

    #[test]
    fn test_mixed_case_display_stop_word() {
        let data = DiagramData::from_raw("Am", "frets x 0 2 2 1 0 Display", 6).unwrap();
        // "Display" should stop fret parsing (case-insensitive)
        assert_eq!(data.frets, vec![-1, 0, 2, 2, 1, 0]);
    }

    #[test]
    fn test_mixed_case_format_stop_word() {
        let data = DiagramData::from_raw("Am", "frets x 0 2 2 1 0 Format", 6).unwrap();
        assert_eq!(data.frets, vec![-1, 0, 2, 2, 1, 0]);
    }

    // --- Fewer fret values than string count (#660) ---

    #[test]
    fn test_fewer_fret_values_than_num_strings() {
        // 3 fret values but num_strings=6 -> strings=6, only 3 have markers
        let data = DiagramData::from_raw("X", "frets 1 2 3", 6).unwrap();
        assert_eq!(data.strings, 6);
        assert_eq!(data.frets.len(), 3);
        // Should render without panic; remaining strings have no markers
        let svg = render_svg(&data);
        assert!(svg.contains("<svg"));
    }

    #[test]
    fn test_fewer_fret_values_inferred() {
        // With from_raw_infer, strings = frets.len()
        let data = DiagramData::from_raw_infer("X", "frets 1 2 3").unwrap();
        assert_eq!(data.strings, 3);
        assert_eq!(data.frets.len(), 3);
    }
}
