# ---- Builder: compile the WASM bundle with dx ----
FROM rust:1 AS builder

RUN rustup target add wasm32-unknown-unknown

# Install the Dioxus CLI (`dx`) via prebuilt binary
RUN curl -L --proto '=https' --tlsv1.2 -sSf \
      https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
RUN cargo binstall dioxus-cli --root /.cargo -y --force
ENV PATH="/.cargo/bin:$PATH"

WORKDIR /app
COPY . .

# Pure client-side build: produces static files under
# target/dx/web/release/web/public (no server binary).
RUN dx bundle --release --package web

# ---- Runtime: tiny Rust static file server ----
FROM ghcr.io/static-web-server/static-web-server:2 AS runtime

COPY --from=builder /app/target/dx/web/release/web/public /public

ENV SERVER_ROOT=/public \
    SERVER_PORT=8888 \
    SERVER_HOST=0.0.0.0
# SPA fallback: serve index.html for unknown routes (client-side router)
ENV SERVER_FALLBACK_PAGE=/public/index.html

EXPOSE 8888
