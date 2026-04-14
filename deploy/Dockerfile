FROM rust:1.87.0-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
RUN useradd -r -s /bin/false mercury
COPY --from=builder /build/target/release/mercury /opt/mercury/mercury
COPY config/default.yaml /opt/mercury/config/default.yaml
RUN mkdir -p /opt/mercury/data /opt/mercury/logs /opt/mercury/keys && \
    chown -R mercury:mercury /opt/mercury
USER mercury
WORKDIR /opt/mercury
EXPOSE 9090
CMD ["/opt/mercury/mercury", "--config", "/opt/mercury/config/default.yaml"]
