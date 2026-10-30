# syntax=docker/dockerfile:1
# The website and the API server in one image, for a single Render web
# service with a disk at /var/data. Build from the repository root:
#   docker build -f deploy/app.Dockerfile -t splitcard-app .
# (render.yaml instead runs them as two services, from web.Dockerfile and
# server.Dockerfile.)

# 1. The API server, as in server.Dockerfile.
FROM rust:1.98.1-slim-bookworm AS chef
RUN cargo install cargo-chef --version 0.1.78 --locked
WORKDIR /src

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY crates crates
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --locked -p splitcard-server --recipe-path recipe.json
COPY Cargo.toml Cargo.lock ./
COPY crates crates
RUN cargo build --release --locked -p splitcard-server --bin splitcard-server

FROM debian:bookworm-20261027-slim AS litestream
ARG TARGETARCH
ARG LITESTREAM_VERSION=0.5.17
ADD https://github.com/benbjohnson/litestream/releases/download/v${LITESTREAM_VERSION}/litestream-${LITESTREAM_VERSION}-linux-x86_64.tar.gz /tmp/amd64.tar.gz
ADD https://github.com/benbjohnson/litestream/releases/download/v${LITESTREAM_VERSION}/litestream-${LITESTREAM_VERSION}-linux-arm64.tar.gz /tmp/arm64.tar.gz
RUN set -eu; \
    echo "cfb371176d164437ae869f8351cfde49bd1804ae71c61923f75c9cba9c9c006d  /tmp/amd64.tar.gz" > /tmp/sums; \
    echo "f8ca4a050095c1efbda2c4365172e61bf9d955ea0d9ac42f448b52e51819baa5  /tmp/arm64.tar.gz" >> /tmp/sums; \
    sha256sum -c /tmp/sums; \
    tar -xzf "/tmp/${TARGETARCH}.tar.gz" -C /usr/local/bin litestream

# 2. The website, as a standalone Next.js server. The browser WebAssembly
# package is committed (crates/splitcard-wasm/pkg), so no Rust is needed here.
FROM node:24.22.2-bookworm-slim AS web
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
COPY next.config.ts tsconfig.json ./
COPY public public
COPY src src
COPY crates/splitcard-wasm/pkg crates/splitcard-wasm/pkg
ENV NEXT_TELEMETRY_DISABLED=1
RUN npm run build

# 3. Both, on a Node runtime.
FROM node:24.22.2-bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --user-group --home-dir /nonexistent --shell /usr/sbin/nologin splitcard \
    # Free instances have no disk: the database then lives in the container
    # and is lost on every restart, unless Litestream restores it.
    && install -d -o splitcard -g splitcard /var/data
WORKDIR /app
COPY --from=web --chown=node:node /app/.next/standalone ./
COPY --from=web --chown=node:node /app/.next/static .next/static
COPY --from=web --chown=node:node /app/public public
COPY --from=builder /src/target/release/splitcard-server /usr/local/bin/splitcard-server
COPY --from=litestream /usr/local/bin/litestream /usr/local/bin/litestream
COPY deploy/litestream.yml /etc/litestream.yml
COPY deploy/server-entrypoint.sh /usr/local/bin/server-entrypoint
COPY deploy/app-entrypoint.sh /usr/local/bin/app-entrypoint
ENV NODE_ENV=production NEXT_TELEMETRY_DISABLED=1 HOSTNAME=0.0.0.0 PORT=10000 \
    DATABASE_URL=sqlite:///var/data/splitcard.db
EXPOSE 10000
# Starts as root only so the server's entrypoint can hand the disk to the
# splitcard user; the website runs as node, the server as splitcard.
ENTRYPOINT ["/usr/local/bin/app-entrypoint"]
