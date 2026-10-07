FROM rust:1-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
# docs/ (repo root) is compiled in via include_str! -- passed as the `docs`
# build context by docker-compose.yml and the release workflow.
COPY --from=docs . /docs
ARG APP_VERSION
ENV APP_VERSION=$APP_VERSION
RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/eunomia-backend /usr/local/bin/eunomia-backend

EXPOSE 8001

CMD ["eunomia-backend"]
