// !!! PLACEHOLDER — DO NOT BUILD WITHOUT RUNNING uniffi-bindgen FIRST !!!
//
// This file lives next to the UniFFI-generated `chordsketch.kt` (note the
// case difference — on Linux they are two separate files; on macOS they
// would collide). The generated file is the one that contains the real
// bindings. This `ChordSketch.kt` file is intentionally empty so that
// the directory exists in git, but the package will not work end-to-end
// until `uniffi-bindgen generate` has been run.
//
// If your `./gradlew test` fails with `cannot load library 'chordsketch_ffi'`
// or `class uniffi.chordsketch.Chordsketch not found`, the binding
// generation step did not run. See `.github/workflows/kotlin.yml` for
// the canonical generate command, or run locally:
//
//   cargo build -p chordsketch-ffi
//   cargo run -p chordsketch-ffi --bin uniffi-bindgen generate \
//     --library target/debug/libchordsketch_ffi.so \
//     --language kotlin \
//     --out-dir packages/kotlin/lib/src/main/kotlin/
//
// A true `#error`-style fail-loud (a top-level statement that won't
// compile) was attempted but ruled out: it would also break CI, where
// `uniffi-bindgen` produces `chordsketch.kt` as a SIBLING file rather
// than overwriting this PascalCase one. See #1076 for the full design
// discussion.
