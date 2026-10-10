# Same toolchain CI and local builds use (CI tracks stable); bump deliberately.
FROM rust:1.95.0-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
# Migrations are compiled in via include_str! (src/migrate.rs).
COPY migrations ./migrations
# docs/ (repo root) is compiled in via include_str! -- passed as the `docs`
# build context by docker-compose.yml and the release workflow.
COPY --from=docs . /docs
ARG APP_VERSION
ENV APP_VERSION=$APP_VERSION
# `default`: cargo uses every core. On a small Docker VM (about 8 GB) the SurrealDB crates can run out of
# memory at full parallelism: build with `--build-arg CARGO_BUILD_JOBS=2`.
ARG CARGO_BUILD_JOBS=default
ENV CARGO_BUILD_JOBS=$CARGO_BUILD_JOBS
RUN cargo build --release --locked

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/eunomia-backend /usr/local/bin/eunomia-backend

# Not root. The only path it writes is /update-status (a bind mount that scripts/updater.sh keeps
# world-writable); everything else it needs is read-only.
RUN groupadd --system --gid 10001 eunomia && useradd --system --uid 10001 --gid 10001 --no-create-home eunomia
USER eunomia

EXPOSE 8001

CMD ["eunomia-backend"]
