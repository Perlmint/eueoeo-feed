FROM rust:1.82-alpine as builder

RUN apk add --no-cache \
        musl-dev \
        ca-certificates && \
    update-ca-certificates

ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
RUN cargo install sqlx-cli --no-default-features --features sqlite,sqlx/runtime-tokio-rustls

WORKDIR /src

ADD \
    Cargo.toml \
    .env \
    ./
ADD src/ ./src
ADD migrations/ ./migrations

ENV PKG_CONFIG_ALL_STATIC=1
RUN sqlx database create && sqlx migrate run

RUN cargo build --release

FROM scratch

COPY --from=builder /src/target/release/eueoeo-feed /app/eueoeo-feed
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

WORKDIR /app
ENV PORT=3000
EXPOSE 3000
ENTRYPOINT [ "/app/eueoeo-feed" ]
