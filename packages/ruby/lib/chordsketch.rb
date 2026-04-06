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

    # Pre-load the native library so UniFFI-generated ffi_lib calls find it.
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

      FFI::DynamicLibrary.open(
        lib_path,
        FFI::DynamicLibrary::RTLD_LAZY | FFI::DynamicLibrary::RTLD_GLOBAL
      )
    end
  end
end

Chordsketch::NativeLoader.load!

# Load the UniFFI-generated bindings (the pre-loaded library will be found).
require_relative "chordsketch_uniffi"
