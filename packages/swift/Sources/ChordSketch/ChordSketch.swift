// !!! PLACEHOLDER — DO NOT BUILD WITHOUT RUNNING uniffi-bindgen FIRST !!!
//
// This file lives next to the UniFFI-generated `chordsketch.swift` (note
// the case difference). The generated file is the one that contains the
// real bindings. This `ChordSketch.swift` file is intentionally empty so
// that the directory exists in git, but the package will not work
// end-to-end until `uniffi-bindgen generate` has been run.
//
// On macOS (case-insensitive HFS+/APFS) the `cp` step in `swift.yml`
// happens to overwrite this PascalCase file, but on Linux/case-sensitive
// filesystems they would be sibling files. Either way, the generated
// `chordsketch.swift` is the source of truth.
//
// If your `swift test` fails with `cannot find chordsketchFFI in scope`
// or `module 'chordsketchFFI' not found`, the binding generation step
// did not run. See `.github/workflows/swift.yml` for the canonical
// generate command, or run locally:
//
//   cargo build -p chordsketch-ffi
//   cargo run -p chordsketch-ffi --bin uniffi-bindgen generate \
//     --library target/debug/libchordsketch_ffi.dylib \
//     --language swift \
//     --out-dir packages/swift/Sources/ChordSketch
//
// A true `#error`-style fail-loud was attempted but ruled out: on
// case-sensitive filesystems the placeholder is NOT overwritten by
// the cp, so the directive would also fire in CI. See #1076 for the
// full design discussion.
