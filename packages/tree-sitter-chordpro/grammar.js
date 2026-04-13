/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

module.exports = grammar({
  name: "chordpro",

  // Carriage returns are ignored (handle \r\n gracefully)
  extras: (_) => [/\r/],

  rules: {
    source_file: ($) => repeat($._line),

    _line: ($) =>
      choice(
        $.comment,
        $.delegate_block,
        $.directive,
        $.content_line,
        $._empty_line,
      ),

    _empty_line: (_) => /\n/,

    // Lines starting with # are comments
    comment: (_) => token(seq("#", /[^\n]*/)),

    // Delegate blocks: {start_of_X} ... {end_of_X}
    // These wrap content like ABC notation, Lilypond, etc.
    delegate_block: ($) =>
      seq(
        $.block_start_directive,
        optional($.block_content),
        $.block_end_directive,
      ),

    block_start_directive: ($) =>
      seq(
        "{",
        field("name", alias(/start_of_[a-zA-Z][a-zA-Z0-9_-]*/, $.directive_name)),
        "}",
        optional("\n"),
      ),

    block_end_directive: ($) =>
      seq(
        "{",
        field("name", alias(/end_of_[a-zA-Z][a-zA-Z0-9_-]*/, $.directive_name)),
        "}",
        optional("\n"),
      ),

    block_content: (_) => repeat1(/[^\n{]*\n/),

    // Directives: {name}, {name: value}, or {name value}
    directive: ($) =>
      seq(
        "{",
        field("name", $.directive_name),
        optional(
          seq(
            token.immediate(/[: ]\s*/),
            field("value", $.directive_value),
          ),
        ),
        "}",
        optional("\n"),
      ),

    // Allows hyphens for selector suffixes (e.g., textfont-piano)
    directive_name: (_) => /[a-zA-Z_][a-zA-Z0-9_-]*/,

    directive_value: (_) => /[^{}]+/,

    // Content lines contain chords and/or lyrics.
    // Trailing newline is optional to handle files without a final newline.
    content_line: ($) =>
      prec.right(seq(repeat1(choice($.chord, $.lyrics)), optional("\n"))),

    // Chord annotation: [Am], [G/B], etc.
    chord: ($) => seq("[", $.chord_name, "]"),

    chord_name: (_) => /[^\[\]\n]+/,

    // Lyric text: any text that is not a chord, directive, or comment.
    // Excludes [, {, }, #, and newline to avoid consuming syntax characters.
    lyrics: (_) => /[^\[\n{}#]+/,
  },
});
