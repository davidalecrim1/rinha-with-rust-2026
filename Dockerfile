FROM rust:alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /app
COPY Cargo.toml Cargo.lock build.rs ./
COPY src       ./src
COPY resources ./resources
# Enable AVX2 and FMA CPU instructions so LLVM auto-vectorizes the brute-force
# KNN inner loop (14 f32 ops per reference vector). Without this, the compiler
# emits scalar code; with AVX2 it processes 8 floats per instruction, bringing
# the 100K-vector search from ~500µs down to ~50-70µs on the test machine.
# The musl target produces a fully static binary with no libc dependency.
ENV RUSTFLAGS="-C target-feature=+avx2,+fma"
RUN cargo build --release --target x86_64-unknown-linux-musl

FROM busybox:musl
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/rinha /rinha
EXPOSE 8080
ENTRYPOINT ["/rinha"]
