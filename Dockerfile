# Multi-stage Dockerfile for building ChordSketch from source.
# For pre-built multi-arch images, see ghcr.io/koedame/chordsketch.

FROM rust:1.85-bookworm AS builder

WORKDIR /build
COPY . .

RUN cargo build --release --locked -p chordsketch && \
    cp target/release/chordsketch /usr/local/bin/chordsketch

# Pinned to a specific date-stamped Debian point release (was floating
# `debian:bookworm-slim`) so a future bookworm patch cannot silently break
# the source-build path. Bump intentionally via Dependabot. See #1070.
FROM debian:bookworm-20260406-slim

RUN useradd --no-create-home --uid 1000 chordsketch
COPY --from=builder /usr/local/bin/chordsketch /usr/local/bin/chordsketch
USER chordsketch

ENTRYPOINT ["/usr/local/bin/chordsketch"]
