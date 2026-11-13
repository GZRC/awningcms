# syntax=docker/dockerfile:1
FROM rust:1-bookworm AS build
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
COPY static ./static
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/build/target \
    cargo test --locked --release && cargo build --locked --release && \
    install -Dm755 target/release/awningcms /out/awningcms

FROM debian:bookworm-slim AS base
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && \
    rm -rf /var/lib/apt/lists/* && \
    groupadd --gid 10001 cms && useradd --uid 10001 --gid cms --no-create-home cms
WORKDIR /app
COPY static /app/static
ENV STATIC_DIR=/app/static PORT=8080
USER 10001:10001
EXPOSE 8080
HEALTHCHECK --interval=15s --timeout=3s --start-period=30s --retries=4 \
    CMD curl -fsS http://127.0.0.1:8080/healthz || exit 1
CMD ["/app/awningcms"]

# Chemin rapide : binaire Linux préalablement compilé et testé localement.
FROM base AS prebuilt
COPY --chmod=755 .build/awningcms /app/awningcms

FROM base AS runtime
COPY --from=build /out/awningcms /app/awningcms
