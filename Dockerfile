# Applicamp image: the `applicamp` server binary and the built web client on
# distroless.
#
#   NODE_AUTH_TOKEN=$(gh auth token) docker build \
#     --secret id=node_auth_token,env=NODE_AUTH_TOKEN \
#     --build-arg GIT_SHA=$(git rev-parse --short HEAD) -t applicamp .
#
# Multi-arch safe (linux/amd64, linux/arm64): no architecture-specific steps.
# Rust dependencies are cached in their own layer with cargo-chef. Queries are
# checked at runtime and migrations are embedded in the binary, so no database
# is needed here.

ARG RUST_IMAGE=rust:1.96-bookworm
ARG CARGO_CHEF_VERSION=0.1.78

# --- web: build the web client (clients/app) ----------------------------------
# @notacamp/ui comes from GitHub Packages, which needs a token even for public
# packages: a BuildKit secret (`--secret id=node_auth_token,env=NODE_AUTH_TOKEN`),
# never a build arg, so it is in no layer and no image history.
FROM --platform=$BUILDPLATFORM node:22-bookworm-slim AS web
ENV COREPACK_ENABLE_DOWNLOAD_PROMPT=0 CI=true
RUN corepack enable
WORKDIR /src/clients
COPY clients/package.json clients/pnpm-lock.yaml clients/pnpm-workspace.yaml clients/.npmrc ./
COPY clients/app/package.json app/package.json
RUN --mount=type=secret,id=node_auth_token,required=true \
    NODE_AUTH_TOKEN="$(cat /run/secrets/node_auth_token)" pnpm install --frozen-lockfile
COPY clients/ ./
RUN pnpm --filter @applicamp/app build

# --- chef: toolchain + cargo-chef ---------------------------------------------
FROM ${RUST_IMAGE} AS chef
ARG CARGO_CHEF_VERSION
# Use the image's own toolchain instead of letting rustup download the one
# named in rust-toolchain.toml (same release).
ENV RUSTUP_TOOLCHAIN=${RUST_VERSION} \
    CARGO_TERM_COLOR=never
RUN cargo install cargo-chef --version ${CARGO_CHEF_VERSION} --locked
WORKDIR /src

# --- planner: compute the dependency recipe ------------------------------------
FROM chef AS planner
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY .cargo .cargo
COPY crates crates
RUN cargo chef prepare --recipe-path recipe.json

# --- builder: build deps (cached), then the server -----------------------------
FROM chef AS builder
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --locked --package applicamp-server --recipe-path recipe.json
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY .cargo .cargo
COPY migrations migrations
COPY crates crates
RUN cargo build --release --locked --package applicamp-server --bin applicamp

# --- runtime: distroless, non-root, no shell ----------------------------------
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
ARG GIT_SHA=""
LABEL org.opencontainers.image.title="applicamp" \
      org.opencontainers.image.description="Applicamp server" \
      org.opencontainers.image.source="https://github.com/notacamp/applicamp" \
      org.opencontainers.image.licenses="LicenseRef-Sustainable-Use" \
      org.opencontainers.image.revision="${GIT_SHA}"
ENV GIT_SHA=${GIT_SHA} \
    APPLICAMP_BIND_ADDR=0.0.0.0:8080 \
    APPLICAMP_LOG_FORMAT=json \
    APPLICAMP_WEB_DIR=/app/web
COPY --from=builder /src/target/release/applicamp /app/applicamp
COPY --from=web /src/clients/app/dist /app/web
USER nonroot:nonroot
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
    CMD ["/app/applicamp", "healthcheck"]
ENTRYPOINT ["/app/applicamp"]
CMD ["serve"]
