use flate2::read::GzDecoder;
use serde::Deserialize;
use std::io::Read;

pub struct FraudIndex {
    vectors: Vec<[f32; 14]>,
    labels: Vec<bool>,
}

#[derive(Deserialize)]
struct RefEntry {
    vector: [f32; 14],
    label: String,
}

impl FraudIndex {
    pub fn build(gz: &[u8]) -> Self {
        let mut decoder = GzDecoder::new(gz);
        let mut json = String::new();
        decoder
            .read_to_string(&mut json)
            .expect("failed to decompress references");

        let entries: Vec<RefEntry> =
            serde_json::from_str(&json).expect("failed to parse references.json");

        let mut vectors = Vec::with_capacity(entries.len());
        let mut labels = Vec::with_capacity(entries.len());
        for entry in entries {
            vectors.push(entry.vector);
            labels.push(entry.label == "fraud");
        }

        FraudIndex { vectors, labels }
    }

    /// Returns the fraud score for query vector `q` using k-NN with k=5.
    ///
    /// Scans all reference vectors, keeps the 5 nearest by squared Euclidean
    /// distance, and returns the fraction of those 5 labeled as fraud (0.0–1.0).
    /// Callers apply a threshold to decide approval (see `FRAUD_THRESHOLD`).
    pub fn search(&self, q: &[f32; 14]) -> f32 {
        let mut neighbors: [(f32, bool); 5] = [(f32::MAX, false); 5];
        let (mut worst_slot, mut worst_dist) = (0, f32::MAX);

        for (vec, &is_fraud) in self.vectors.iter().zip(self.labels.iter()) {
            let dist = dist_sq(q, vec);
            if dist < worst_dist {
                neighbors[worst_slot] = (dist, is_fraud);
                (worst_slot, worst_dist) = find_worst_neighbor(&neighbors);
            }
        }

        neighbors.iter().filter(|(_, is_fraud)| *is_fraud).count() as f32 / 5.0
    }
}

/// Returns the index and distance of the farthest neighbor in the current top-5.
/// Used to identify the eviction candidate when a closer vector is found.
fn find_worst_neighbor(neighbors: &[(f32, bool); 5]) -> (usize, f32) {
    neighbors
        .iter()
        .enumerate()
        .fold((0, f32::NEG_INFINITY), |(wi, wd), (i, &(d, _))| {
            if d > wd {
                (i, d)
            } else {
                (wi, wd)
            }
        })
}

/// Squared Euclidean distance. Fixed-size array lets LLVM vectorize with AVX2.
#[inline(always)]
fn dist_sq(a: &[f32; 14], b: &[f32; 14]) -> f32 {
    let mut sum = 0.0f32;
    for i in 0..14 {
        let d = a[i] - b[i];
        sum += d * d;
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    fn make_gz(entries: &[([f32; 14], &str)]) -> Vec<u8> {
        let records: Vec<String> = entries
            .iter()
            .map(|(v, label)| {
                let vec_str = v
                    .iter()
                    .map(|f| f.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                format!("{{\"vector\":[{}],\"label\":\"{}\"}}", vec_str, label)
            })
            .collect();
        let json = format!("[{}]", records.join(","));
        let mut enc = GzEncoder::new(Vec::new(), Compression::default());
        enc.write_all(json.as_bytes()).unwrap();
        enc.finish().unwrap()
    }

    const ZERO: [f32; 14] = [0.0; 14];

    #[test]
    fn search_all_fraud() {
        let gz = make_gz(&[
            (ZERO, "fraud"),
            (ZERO, "fraud"),
            (ZERO, "fraud"),
            (ZERO, "fraud"),
            (ZERO, "fraud"),
        ]);
        let index = FraudIndex::build(&gz);
        assert_eq!(index.search(&ZERO), 1.0);
    }

    #[test]
    fn search_all_legit() {
        let gz = make_gz(&[
            (ZERO, "legit"),
            (ZERO, "legit"),
            (ZERO, "legit"),
            (ZERO, "legit"),
            (ZERO, "legit"),
        ]);
        let index = FraudIndex::build(&gz);
        assert_eq!(index.search(&ZERO), 0.0);
    }

    #[test]
    fn search_mixed_3_fraud_2_legit() {
        let gz = make_gz(&[
            (ZERO, "fraud"),
            (ZERO, "fraud"),
            (ZERO, "fraud"),
            (ZERO, "legit"),
            (ZERO, "legit"),
        ]);
        let index = FraudIndex::build(&gz);
        assert_eq!(index.search(&ZERO), 0.6);
    }

    #[test]
    fn search_nearest_neighbors_win() {
        // 5 fraud vectors close to origin; 1 legit far away — top-5 must be all fraud
        let close_fraud = [0.01f32; 14];
        let far_legit = [0.9f32; 14];
        let gz = make_gz(&[
            (close_fraud, "fraud"),
            (close_fraud, "fraud"),
            (close_fraud, "fraud"),
            (close_fraud, "fraud"),
            (close_fraud, "fraud"),
            (far_legit, "legit"),
        ]);
        let index = FraudIndex::build(&gz);
        assert_eq!(index.search(&ZERO), 1.0);
    }
}
