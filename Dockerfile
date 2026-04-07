# Multi-stage Dockerfile for building ChordSketch from source.
# For pre-built multi-arch images, see ghcr.io/koedame/chordsketch.

# Builder stage pinned to its sha256 manifest list digest for the same
# reason the runtime stage is below: a tag republish on Docker Hub could
# otherwise produce a tampered builder that quietly compiles malicious
# binary into the trusted runtime layer. The tag is kept alongside the
# digest so Dependabot can correlate the bump. See #1103.
FROM rust:1.85-bookworm@sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4 AS builder

WORKDIR /build
COPY . .

RUN cargo build --release --locked -p chordsketch && \
    cp target/release/chordsketch /usr/local/bin/chordsketch

# Pinned to a specific date-stamped Debian point release AND its sha256
# manifest list digest (was floating `debian:bookworm-slim`) so a tag
# republish or a future bookworm patch cannot silently change the image
# we ship. The tag is kept alongside the digest so Dependabot can
# correlate the bump and humans can read the version at a glance.
# Bump intentionally via Dependabot. See #1070, #1100.
FROM debian:bookworm-20260406-slim@sha256:4724b8cc51e33e398f0e2e15e18d5ec2851ff0c2280647e1310bc1642182655d

RUN useradd --no-create-home --uid 1000 chordsketch
COPY --from=builder /usr/local/bin/chordsketch /usr/local/bin/chordsketch
USER chordsketch

ENTRYPOINT ["/usr/local/bin/chordsketch"]
