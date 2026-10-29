# syntax=docker/dockerfile:1
# The Split Card server. Build from the repository root:
#   docker build -f deploy/server.Dockerfile -t splitcard-server .

FROM rust:1.98.1-slim-bookworm AS chef
RUN cargo install cargo-chef --version 0.1.78 --locked
WORKDIR /src

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY crates crates
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
# Dependencies first, so a source change does not rebuild them.
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

FROM debian:bookworm-20261027-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --user-group --home-dir /nonexistent --shell /usr/sbin/nologin splitcard
COPY --from=builder /src/target/release/splitcard-server /usr/local/bin/splitcard-server
COPY --from=litestream /usr/local/bin/litestream /usr/local/bin/litestream
COPY deploy/litestream.yml /etc/litestream.yml
COPY deploy/server-entrypoint.sh /usr/local/bin/server-entrypoint
ENV DATABASE_URL=sqlite:///var/data/splitcard.db
EXPOSE 10000
# The entrypoint starts as root only to chown the disk, then runs the
# server as the splitcard user.
ENTRYPOINT ["/usr/local/bin/server-entrypoint"]
