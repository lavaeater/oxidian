# ---- Builder: compile the WASM bundle with dx ----
# Pinned to 1.95: rustc 1.96 fails to compile rhai 1.23.6 (a dioxus-cli dep),
# and 1.95 is the toolchain this project builds with locally.
FROM rust:1.95 AS builder

RUN rustup target add wasm32-unknown-unknown

# Install cargo-binstall, then the Dioxus CLI (`dx`).
# No prebuilt exists for the alpha, so binstall compiles it from source —
# cache the registry/git dirs so deps aren't re-downloaded on rebuilds.
RUN curl -L --proto '=https' --tlsv1.2 -sSf \
      https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
ENV PATH="/.cargo/bin:$PATH"
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo binstall dioxus-cli@0.8.0-alpha.0 --root /.cargo -y --force

WORKDIR /app
COPY . .

# Pure client-side build: produces static files under
# target/dx/web/release/web/public (no server binary).
# target/ is a cache mount (not persisted in the layer), so copy the
# bundle out to /public-out, which the runtime stage COPYs from.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    dx bundle --release --package web \
 && mkdir -p /public-out \
 && cp -r target/dx/web/release/web/public/. /public-out/

# ---- Runtime: tiny Rust static file server ----
FROM ghcr.io/static-web-server/static-web-server:2 AS runtime

COPY --from=builder /public-out /public

ENV SERVER_ROOT=/public \
    SERVER_PORT=5173 \
    SERVER_HOST=0.0.0.0
# SPA fallback: serve index.html for unknown routes (client-side router)
ENV SERVER_FALLBACK_PAGE=/public/index.html

EXPOSE 5173
