// External scanner for tree-sitter-chordpro.
//
// Purpose: restrict `#` comment recognition to column 0 only.
// Per the ChordPro specification, `#` is a comment marker only at the
// beginning of a line.  Mid-line `#` (e.g. `Play the C# note`) is
// ordinary text.

#include "tree_sitter/parser.h"

enum TokenType {
  COMMENT,
};

void *tree_sitter_chordpro_external_scanner_create(void) { return NULL; }

void tree_sitter_chordpro_external_scanner_destroy(void *payload) {}

unsigned tree_sitter_chordpro_external_scanner_serialize(void *payload,
                                                         char *buffer) {
  return 0;
}

void tree_sitter_chordpro_external_scanner_deserialize(void *payload,
                                                        const char *buffer,
                                                        unsigned length) {}

bool tree_sitter_chordpro_external_scanner_scan(void *payload, TSLexer *lexer,
                                                 const bool *valid_symbols) {
  if (!valid_symbols[COMMENT]) {
    return false;
  }

  // A comment is only valid at the very beginning of a line (column 0).
  if (lexer->get_column(lexer) != 0) {
    return false;
  }

  if (lexer->lookahead != '#') {
    return false;
  }

  // Consume `#` and the rest of the line.
  lexer->advance(lexer, false);
  while (lexer->lookahead != '\n' && lexer->lookahead != '\0' &&
         !lexer->eof(lexer)) {
    lexer->advance(lexer, false);
  }

  lexer->mark_end(lexer);
  lexer->result_symbol = COMMENT;
  return true;
}
