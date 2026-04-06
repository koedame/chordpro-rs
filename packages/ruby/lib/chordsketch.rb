# frozen_string_literal: true

# ChordPro file format parser and renderer.
#
# This gem provides native bindings to the ChordSketch Rust library
# via UniFFI-generated Ruby code.
#
# @example Render ChordPro as text
#   text = Chordsketch.parse_and_render_text("{title: Hello}\n[C]Hello", nil, nil)
#
# @example Render with transposition
#   text = Chordsketch.parse_and_render_text(input, "guitar", 2)

# The actual module is defined in the UniFFI-generated chordsketch.rb.
# For local development, generate bindings with:
#   cargo run -p chordsketch-ffi --bin uniffi-bindgen generate \
#     --library target/debug/libchordsketch_ffi.so \
#     --language ruby \
#     --out-dir packages/ruby/lib/
