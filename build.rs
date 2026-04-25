use flate2::read::GzDecoder;
use serde::Deserialize;
use std::io::{Read, Write};
use std::path::Path;

const SCALE: f32 = 8192.0;
const STRIDE: usize = 16;

#[derive(Deserialize)]
struct RefEntry {
    vector: [f32; 14],
    label: String,
}

fn quantize_dim(i: usize, v: f32) -> i16 {
    match i {
        // Dims 5-6 allow -1 sentinel (no prior transaction)
        5 | 6 => (v.clamp(-1.0, 1.0) * SCALE).round() as i16,
        // Binary flags: is_online, card_present, unknown_merchant
        9 | 10 | 11 => {
            if v > 0.5 {
                SCALE as i16
            } else {
                0
            }
        }
        // All other continuous dims are in [0, 1]
        _ => (v.clamp(0.0, 1.0) * SCALE + 0.5) as i16,
    }
}

fn main() {
    println!("cargo:rerun-if-changed=resources/references.json.gz");

    let gz = include_bytes!("resources/references.json.gz");
    let mut decoder = GzDecoder::new(&gz[..]);
    let mut json = String::new();
    decoder
        .read_to_string(&mut json)
        .expect("failed to decompress references.json.gz");

    let entries: Vec<RefEntry> =
        serde_json::from_str(&json).expect("failed to parse references.json");

    let n = entries.len();

    // Build quantized reference buffer: n rows x 16 i16 values (14 dims + 2 padding zeros)
    let mut refs_buf: Vec<u8> = Vec::with_capacity(n * STRIDE * 2);
    let mut labels_buf: Vec<u8> = Vec::with_capacity(n);

    for entry in &entries {
        let mut row = [0i16; STRIDE];
        for (i, &v) in entry.vector.iter().enumerate() {
            row[i] = quantize_dim(i, v);
        }
        // Write row as little-endian i16 bytes
        for &val in &row {
            refs_buf.extend_from_slice(&val.to_le_bytes());
        }
        labels_buf.push(if entry.label == "fraud" { 1 } else { 0 });
    }

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let out_path = Path::new(&out_dir);

    let mut f = std::fs::File::create(out_path.join("refs.bin")).expect("failed to create refs.bin");
    f.write_all(&refs_buf).expect("failed to write refs.bin");

    let mut f =
        std::fs::File::create(out_path.join("labels.bin")).expect("failed to create labels.bin");
    f.write_all(&labels_buf)
        .expect("failed to write labels.bin");

    // Write the count so index.rs knows how many rows exist
    let mut f =
        std::fs::File::create(out_path.join("count.txt")).expect("failed to create count.txt");
    write!(f, "{}", n).expect("failed to write count.txt");
}
