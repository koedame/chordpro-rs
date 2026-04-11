// swift-tools-version: 5.9

import PackageDescription

let package = Package(
    name: "ChordSketch",
    platforms: [
        .macOS(.v12),
        .iOS(.v15),
    ],
    products: [
        .library(
            name: "ChordSketch",
            targets: ["ChordSketch"]
        ),
    ],
    targets: [
        .binaryTarget(
            name: "chordsketchFFI",
            url: "https://github.com/koedame/chordsketch/releases/download/v0.2.0/chordsketch-xcframework.zip",
            checksum: "a5a6fee17846b2caafe61a2b896263f3f71b008967a2001c8474e567e1c24d6d"
        ),
        .target(
            name: "ChordSketch",
            dependencies: ["chordsketchFFI"],
            path: "Sources/ChordSketch",
            linkerSettings: [
                // chordsketch-render-pdf depends on flate2 which uses
                // the system zlib for compression on Apple platforms.
                .linkedLibrary("z"),
            ]
        ),
        .testTarget(
            name: "ChordSketchTests",
            dependencies: ["ChordSketch"],
            path: "Tests"
        ),
    ]
)
