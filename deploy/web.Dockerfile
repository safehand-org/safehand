# syntax=docker/dockerfile:1
# The Split Card website, which also proxies /api to the server. Build from
# the repository root:
#   docker build -f deploy/web.Dockerfile -t splitcard-web .

# 1. The browser WebAssembly package, built as in CI.
FROM rust:1.98.1-slim-bookworm AS wasm
# secp256k1-sys compiles C for wasm32, which needs clang.
RUN apt-get update \
    && apt-get install -y --no-install-recommends clang \
    && rm -rf /var/lib/apt/lists/*
RUN cargo install wasm-pack --version 0.15.0 --locked
WORKDIR /src
# The toolchain file adds the wasm32 target and llvm-tools.
COPY rust-toolchain.toml Cargo.toml Cargo.lock ./
COPY crates crates
# llvm-ar gives the wasm32 C archive a symbol index the linker can read.
RUN export AR_wasm32_unknown_unknown="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin/llvm-ar" \
    && wasm-pack build crates/splitcard-wasm --target web

# 2. The Next.js app, as a standalone server.
FROM node:24.22.2-bookworm-slim AS web
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
COPY next.config.ts tsconfig.json next-env.d.ts ./
COPY public public
COPY src src
COPY --from=wasm /src/crates/splitcard-wasm/pkg crates/splitcard-wasm/pkg
ENV NEXT_TELEMETRY_DISABLED=1
RUN npm run build

# 3. A small runtime with only the standalone output.
FROM node:24.22.2-bookworm-slim
WORKDIR /app
ENV NODE_ENV=production NEXT_TELEMETRY_DISABLED=1 HOSTNAME=0.0.0.0 PORT=10000
COPY --from=web --chown=node:node /app/.next/standalone ./
COPY --from=web --chown=node:node /app/.next/static .next/static
COPY --from=web --chown=node:node /app/public public
USER node
EXPOSE 10000
CMD ["node", "server.js"]
