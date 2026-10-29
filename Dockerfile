# syntax=docker/dockerfile:1
# Build only from the allowlisted context. Original artwork and local web/pkg
# are excluded; generate release bindings rather than ship a debug WASM.
FROM rust:1.98.1-bookworm AS wasm-build
WORKDIR /build
RUN rustup target add wasm32-unknown-unknown \
    && cargo install wasm-bindgen-cli --version 0.2.129 --locked
COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/
ENV RUSTFLAGS="--remap-path-prefix=/build=/project --remap-path-prefix=/usr/local/cargo=/cargo-home --remap-path-prefix=/usr/local/rustup=/rustup-home"
# web-release: fat LTO in one codegen unit (see Cargo.toml), for a smaller download.
RUN cargo build --locked --profile web-release --target wasm32-unknown-unknown --bin naifrog-duel \
    && wasm-bindgen --target web --no-typescript \
       --out-name naifrog_duel --out-dir /out/web/pkg \
       --remove-name-section --remove-producers-section \
       target/wasm32-unknown-unknown/web-release/naifrog-duel.wasm \
    && gzip -9 -k -n /out/web/pkg/naifrog_duel_bg.wasm
COPY web/index.html web/main.js web/network.js web/style.css /out/web/

# Headless native server: deliberately exclude the default Bevy game feature.
FROM rust:1.98.1-bookworm AS server-build
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/
ENV RUSTFLAGS="--remap-path-prefix=/build=/project --remap-path-prefix=/usr/local/cargo=/cargo-home --remap-path-prefix=/usr/local/rustup=/rustup-home"
RUN cargo build --locked --release --no-default-features --features server --bin naifrog-server \
    && install -D -m 0755 target/release/naifrog-server /out/naifrog-server

FROM debian:bookworm-slim AS runtime
# Only the standard native compiler runtime; no renderer, audio, windowing,
# GPU libraries, Rust toolchain, source tree, or original art in this stage.
RUN apt-get update \
    && apt-get install -y --no-install-recommends libgcc-s1 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 naifrog \
    && useradd --uid 10001 --gid 10001 --no-create-home --shell /usr/sbin/nologin naifrog
WORKDIR /app
COPY --from=server-build --chmod=0555 /out/naifrog-server /app/naifrog-server
COPY --from=wasm-build /out/web/ /app/web/
COPY assets/processed/ /app/assets/processed/
COPY assets/ART_PROVENANCE.json /app/assets/
COPY LICENSE NOTICE.md assets/UNICODE-LICENSE.txt /app/
RUN chmod -R a=rX /app && test ! -e /app/assets/original
ENV NAIFROG_BIND=0.0.0.0:18080 \
    NAIFROG_ASSET_ROOT=/app \
    NAIFROG_MAX_ROOMS=16 \
    NAIFROG_EGRESS_MB_PER_HOUR=512 \
    NAIFROG_ALLOW_INSECURE_PUBLIC_HTTP=0
USER 10001:10001
# Image metadata only: Compose does NOT publish this port to the host.
EXPOSE 18080
# The server handles Ctrl-C for graceful shutdown.
STOPSIGNAL SIGINT
ENTRYPOINT ["/app/naifrog-server"]
