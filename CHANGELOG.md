# Changelog

## v0.4.0
- Quantize 100K reference vectors from f32 to i16 (scale 8192) at compile time via `build.rs`, shrinking the scan buffer from 5.6 MB to 3.2 MB
- Replace runtime JSON decompression with binary blob loading — zero parse cost at startup
- p99 improved from 109ms → 79ms locally on linux/amd64 with 0% error rate
- `make release` now requires explicit `VERSION=vX.Y.Z` argument instead of auto-incrementing the patch

## v0.1.2
- Add k6 load test script, replace shell-based example-payloads with Python
- Include VUs and duration in load test result filenames

## v0.1.1
- Offload KNN search to `spawn_blocking` — frees tokio thread for I/O under concurrent load
- Set `worker_processes 1` in nginx — prevents CPU starvation with 0.05 CPU budget
- p99 improved from 291ms → 129ms; score from 3535 → 3888

## v0.1.0
- Brute-force KNN over 100K reference vectors with AVX2/FMA auto-vectorization
- Full fraud detection API: `/ready` healthcheck + `/fraud-score` endpoint
- Static binary via `FROM busybox:musl`, resource files embedded at compile time
