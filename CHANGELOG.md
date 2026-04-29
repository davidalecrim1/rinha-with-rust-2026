# Changelog

## v0.7.1
- Port i16 fixed-point quantization (SCALE=8192) with 16-byte packed rows, replacing f32 brute-force
- SSE2 SIMD distance kernel for 6 continuous dims; PartialDists lookup table for 5 discrete dims
- Insertion-sort top-5 with monotonically tightening bound; 6 pre-computed JSON responses eliminate serde_json on hot path

## v0.7.0
- Updated to 3M reference dataset
- Replace TCP with Unix domain sockets between nginx and api instances
- Increase nginx `worker_connections` to 1024 to avoid connection queuing under load

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
