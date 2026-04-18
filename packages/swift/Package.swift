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
            url: "https://github.com/koedame/chordsketch/releases/download/v0.2.2/chordsketch-xcframework.zip",
            checksum: "1c0116fd5c942533f9d76ee4534dca1457fa8a078b18eda411770a02d76e4934"
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
