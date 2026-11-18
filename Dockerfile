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
    cargo binstall dioxus-cli@0.8.0-alpha.3 --root /.cargo -y --force

WORKDIR /app
COPY . .

# Pure client-side build: produces static files under
# target/dx/web/release/web/public (no server binary).
# target/ is a cache mount (not persisted in the layer), so copy the
# bundle out to /public-out, which the runtime stage COPYs from.
# The mount has its own id: the default id is the target path, which other
# projects' Dockerfiles (craboot, decapod) also use, so they would share it.
# Because target/ persists between builds, wipe dx's bundle output first:
# a stale index.html left there keeps pointing at the previous build's
# hashed JS/wasm, so the image would silently serve the old app.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,id=oxidian-target,target=/app/target \
    rm -rf target/dx \
 && dx bundle --release --package web \
 && mkdir -p /public-out \
 && cp -r target/dx/web/release/web/public/. /public-out/

# ---- Runtime: tiny Rust static file server ----
FROM ghcr.io/static-web-server/static-web-server:2 AS runtime

COPY --from=builder /public-out /public
COPY static-web-server.toml /static-web-server.toml

# Cache-Control comes from static-web-server.toml, not the server's defaults
# (which let browsers keep index.html for a day).
# No SPA fallback page: the app has no client-side routes, and a fallback
# would answer a request for a missing .wasm with index.html and a 200.
ENV SERVER_ROOT=/public \
    SERVER_PORT=5173 \
    SERVER_HOST=0.0.0.0 \
    SERVER_CONFIG_FILE=/static-web-server.toml \
    SERVER_CACHE_CONTROL_HEADERS=false

EXPOSE 5173
