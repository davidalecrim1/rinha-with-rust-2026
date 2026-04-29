# rinha-with-rust-2026

Rust submission for rinha-de-backend-2026 — fraud detection via vector search.

## Why Rust

The scoring formula rewards p99 ≤ 1ms with maximum latency points. Go's GC introduces non-deterministic tail latency that is tunable but not eliminable. Rust has zero GC — latency is fully deterministic. On a slow test machine (Mac Mini 2014, 2.6 GHz), this difference is real under load.

See `docs/rust-vs-go.md` for the full trade-off analysis. Build and validate correctness with the Go submission first, then use Rust to push p99 lower.

## Challenge summary

Build a fraud detection API that:
1. Receives a card transaction payload
2. Vectorizes it into 14 f32 dimensions (normalization rules in `rinha-de-backend-2026/docs/en/DETECTION_RULES.md`)
3. Finds the 5 nearest neighbors in a 1M-vector reference dataset using Euclidean distance
4. Returns `approved = fraud_score < 0.6` where `fraud_score = frauds_among_5 / 5`

Full spec: `rinha-de-backend-2026/docs/en/`

## API contract

- `GET /ready` — return 503 until HNSW index is built, then 200
- `POST /fraud-score` — receive transaction, return `{ "approved": bool, "fraud_score": float }`
- Internal port: 8080 (nginx on 9999 forwards here)

## Architecture

```
nginx (0.05 CPU / 15MB)  ← listens on :9999, round-robin
  ├── api1 (0.475 CPU / 167MB)  ← listens on :8080
  └── api2 (0.475 CPU / 167MB)  ← listens on :8080
```

## Design decisions (shared with Go submission)

| Decision | Choice | Reason |
|---|---|---|
| Vector type | i16 packed rows (16 bytes/row) | Halves bandwidth vs f32; 6 continuous dims in bytes 0–11, 5 discrete dims bit-packed in bytes 12–14 as dictionary indices, 3 binary dims as bits, label in byte 15. SCALE=8192 |
| Vector search | Brute-force KNN + SSE2 i16 SIMD + insertion-sort top-5 | ~1M rows × 16 bytes = 16 MB; SSE2 `_mm_madd_epi16` computes 6-dim squared distance in ~10 cycles; insertion sort keeps bound tight, skipping rows early |
| Resource files | Pre-packed binary embedded at compile time (build.rs) | No runtime decompression or JSON parsing — instant startup, smaller hot-path binary |
| Docker | Multi-stage → `FROM scratch` | Tiny final image, statically linked binary |
| nginx | Unix domain sockets (`server unix:/var/run/apiN.sock`) | Bypasses TCP stack entirely; configured in nginx.conf + docker-compose volumes |

## Rust-specific decisions

| Decision | Choice | Reason |
|---|---|---|
| HTTP framework | `axum` | tokio-native, clean serde integration, tower overhead negligible at this scale |
| KNN search | Brute-force over `Vec<[u8; 16]>` packed rows, SSE2 via `-C target-cpu=haswell` | i16 SIMD inner loop; dictionary lookup for discrete dims eliminates per-row arithmetic; no external crate, deterministic recall |
| Top-5 selection | Insertion-sort sorted array, bound = `neighbors[4].0`, tightens monotonically | Rows failing the bound check are skipped before computing fraud label; no rescan of 5 slots |
| Response serialization | 6 pre-computed `Vec<u8>` responses built at startup | Only 6 outcomes (0–5 fraud neighbors); eliminates serde_json on every request |
| Async runtime | `tokio`, `worker_threads = 1` | Matches 0.475 CPU quota; eliminates thread contention, same reasoning as Go's `GOMAXPROCS=1` |
| Cross-compilation | Docker multi-stage builder | Build inside `FROM rust:alpine`; no host toolchain needed, matches competition infra |

## Module contract

Business logic must not leak into handlers. Each module has a single responsibility:

- **`main.rs`**: wires modules, loads embedded resources, builds pre-computed responses, sets the readiness flag
- **`handler.rs`**: deserialize → call `vectorizer::vectorize()` → call `index::search()` → return pre-computed response bytes. No scoring logic.
- **`vectorizer.rs`**: transaction payload → `[f32; 14]`. Pure data transformation.
- **`index.rs`**: owns the packed `Vec<[u8; 16]>` reference buffer. Exposes `fn search(vector: &[f32; 14]) -> u8` returning fraud neighbor count (0–5). Swap the search strategy by touching only this file.
- **`packed_ref.rs`**: 16-byte row encoding — 6 continuous dims as i16 (bytes 0–11), 5 discrete dims as bit-packed dictionary indices (bytes 12–14), 3 binary dims as bits, 1 label byte. Pre-computed partial distances (`PartialDists`) eliminate per-row arithmetic for low-cardinality dims.
- **`simd.rs`**: SSE2 distance kernel for the 6 continuous dims.

## Project structure

```
rinha-with-rust-2026/
├── src/
│   ├── main.rs          # startup, runtime config, readiness flag
│   ├── handler.rs       # axum handlers for /ready and /fraud-score
│   ├── vectorizer.rs    # 14-dim normalization
│   ├── index.rs         # packed-row k-NN, exposes search(vector) -> u8
│   ├── packed_ref.rs    # 16-byte row format, PartialDists, dicts (from build.rs)
│   └── simd.rs          # SSE2 distance kernel
├── build.rs             # packs references.json.gz → packed_refs.bin at compile time
├── resources/
│   ├── references.json.gz
│   ├── mcc_risk.json
│   └── normalization.json
├── Dockerfile
├── Cargo.toml
└── CLAUDE.md
```

## Vectorization notes

Same rules as Go submission:
- `minutes_since_last_tx`: delta between `requested_at` and `last_transaction.timestamp` in minutes, clamped. -1 sentinel only when `last_transaction` is null.
- `unknown_merchant`: 1 if `merchant.id` not in `customer.known_merchants`, else 0.
- `mcc_risk`: look up `merchant.mcc`, default 0.5 if not found.
- All values clamped to [0.0, 1.0] except indices 5 and 6 (-1 sentinel).

## Resource files

Copy from `rinha-de-backend-2026/resources/` into `resources/`:

- `references.json.gz` — 1M labeled vectors (fraud/legit), ~16MB gzipped
- `mcc_risk.json` — MCC code → risk score mapping
- `normalization.json` — constants for the 14-dimension normalization formulas

## Load test dataset

`scripts/test-data.json` is gitignored (22 MB). Run `make fetch-test-data` after a fresh clone to copy it from the `rinha-de-backend-2026` submodule.

## Submission structure

Two branches required:
- `main` — source code
- `submission` — only `docker-compose.yml`, `nginx.conf`, `info.json`

Docker images must be public and compatible with `linux/amd64`.

## Scoring

- `score_p99`: logarithmic, +1000 per 10x improvement. Ceiling at ≤1ms (+3000), floor at >2000ms (-3000).
- `score_det`: FP weight 1, FN weight 3, HTTP error weight 5. Cutoff at >15% failure rate → -3000.
- `final_score = score_p99 + score_det`, range [-6000, +6000].

HTTP 500s are the worst outcome — weight 5 and count toward the failure rate cutoff.

## Rust practices

**Panics**: `.expect()` / `.unwrap()` are acceptable only at startup on embedded data (e.g., `include_bytes!` resources parsed in `main`). Any code reachable from a live request must not panic — return an error type instead. A panic on a malformed request means connection reset for the client and a weight-5 penalty in scoring.

**Parse at the boundary**: validate and parse external input in the serde types, not in business logic. Use `DateTime<FixedOffset>` rather than `String` for timestamps so that invalid dates are rejected at deserialization with a clean 422, before reaching `vectorize`.

**Safe startup panics must have `.expect("message")`** — never bare `.unwrap()` on embedded resource parsing, so crash messages are actionable.
