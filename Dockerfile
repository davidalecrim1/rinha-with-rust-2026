FROM rust:alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /app
COPY Cargo.toml Cargo.lock build.rs ./
COPY src       ./src
COPY resources ./resources
# target-cpu=haswell enables SSE2 (always present on x86_64) for the SIMD
# i16 distance kernel. The musl target produces a fully static binary.
ENV RUSTFLAGS="-C target-cpu=haswell"
RUN cargo build --release --target x86_64-unknown-linux-musl

FROM busybox:musl
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/rinha /rinha
EXPOSE 8080
ENTRYPOINT ["/rinha"]
