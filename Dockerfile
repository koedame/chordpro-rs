# Multi-stage Dockerfile for building ChordSketch from source.
# For pre-built multi-arch images, see ghcr.io/koedame/chordsketch.

FROM rust:1.85-bookworm AS builder

WORKDIR /build
COPY . .

RUN cargo build --release --locked -p chordsketch && \
    cp target/release/chordsketch /usr/local/bin/chordsketch

FROM debian:bookworm-slim

RUN useradd --no-create-home --uid 1000 chordsketch
COPY --from=builder /usr/local/bin/chordsketch /usr/local/bin/chordsketch
USER chordsketch

ENTRYPOINT ["/usr/local/bin/chordsketch"]
