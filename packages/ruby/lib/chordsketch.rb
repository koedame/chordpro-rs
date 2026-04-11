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

require "ffi"
require "rbconfig"

# Detect the platform and pre-load the correct native library before
# requiring the UniFFI-generated bindings.
module Chordsketch
  # @api private
  module NativeLoader
    PLATFORM_MAP = {
      /x86_64.*linux/                 => "x86_64-linux",
      /aarch64.*linux/                => "aarch64-linux",
      /arm64.*darwin|aarch64.*darwin/ => "aarch64-darwin",
      /x86_64.*darwin/                => "x86_64-darwin",
      /x64.*mingw|x64.*mswin/        => "x86_64-windows",
    }.freeze

    # Detect the platform-specific subdirectory for native libraries.
    def self.detect_platform_dir
      arch = RbConfig::CONFIG["arch"]
      PLATFORM_MAP.each do |pattern, dir|
        return dir if arch.match?(pattern)
      end
      raise "Unsupported platform: #{arch}. " \
            "ChordSketch supports x86_64/aarch64 Linux, macOS, and x86_64 Windows."
    end

    # Resolve and validate the absolute path to the platform-specific
    # native library, then expose it as `Chordsketch::NATIVE_LIB_PATH`
    # so the UniFFI-generated bindings can pass it to `ffi_lib`.
    #
    # The previous approach of pre-loading via `FFI::DynamicLibrary.open`
    # with `RTLD_GLOBAL` was unreliable: ffi gem 1.17+ explicitly opens
    # its own handle by name in `ffi_lib` and ignores already-loaded
    # global handles, so the bindings would fail with "Could not open
    # library 'chordsketch_ffi'" even after a successful pre-load. See
    # #1082.
    def self.load!
      platform_dir = detect_platform_dir
      lib_dir = File.join(File.dirname(__FILE__), platform_dir)
      lib_name = FFI.map_library_name("chordsketch_ffi")
      lib_path = File.join(lib_dir, lib_name)

      unless File.exist?(lib_path)
        raise "Native library not found at #{lib_path}. " \
              "Ensure the gem was built for your platform (#{platform_dir}). " \
              "For local development, generate bindings with:\n" \
              "  cargo run -p chordsketch-ffi --bin uniffi-bindgen generate \\\n" \
              "    --library target/debug/libchordsketch_ffi.so \\\n" \
              "    --language ruby --out-dir packages/ruby/lib/"
      end

      Chordsketch.const_set(:NATIVE_LIB_PATH, lib_path)
    end
  end
end

Chordsketch::NativeLoader.load!

# Load the UniFFI-generated bindings. Their `ffi_lib` line is rewritten
# to reference `Chordsketch::NATIVE_LIB_PATH` (set above) by a Python script
# in the CI workflow. See `.github/workflows/ruby.yml` and #1082.
require_relative "chordsketch_uniffi"
