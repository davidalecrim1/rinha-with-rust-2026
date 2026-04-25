const STRIDE: usize = 16;
const N: usize = include!(concat!(env!("OUT_DIR"), "/count.txt"));
const SCALE: f32 = 8192.0;

static REFS_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/refs.bin"));
static LABELS_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/labels.bin"));

pub struct FraudIndex {
    refs: Vec<[i16; STRIDE]>,
    labels: Vec<u8>,
}

impl FraudIndex {
    pub fn new() -> Self {
        assert!(REFS_BYTES.len() == N * STRIDE * 2);
        assert!(LABELS_BYTES.len() == N);

        let mut refs = Vec::with_capacity(N);
        for row_bytes in REFS_BYTES.chunks_exact(STRIDE * 2) {
            let mut row = [0i16; STRIDE];
            for (i, chunk) in row_bytes.chunks_exact(2).enumerate() {
                row[i] = i16::from_le_bytes([chunk[0], chunk[1]]);
            }
            refs.push(row);
        }

        FraudIndex {
            refs,
            labels: LABELS_BYTES.to_vec(),
        }
    }

    pub fn search(&self, q: &[i16; STRIDE]) -> f32 {
        let mut neighbors: [(i32, usize); 5] = [(i32::MAX, 0); 5];
        let (mut worst_slot, mut worst_dist) = (0, i32::MAX);

        for (idx, row) in self.refs.iter().enumerate() {
            let dist = dist_sq(q, row);
            if dist < worst_dist {
                neighbors[worst_slot] = (dist, idx);
                let (ws, wd) = find_worst_neighbor(&neighbors);
                worst_slot = ws;
                worst_dist = wd;
            }
        }

        let fraud_count = neighbors
            .iter()
            .filter(|(_, idx)| self.labels[*idx] == 1)
            .count();
        fraud_count as f32 / 5.0
    }
}

fn find_worst_neighbor(neighbors: &[(i32, usize); 5]) -> (usize, i32) {
    neighbors
        .iter()
        .enumerate()
        .fold((0, i32::MIN), |(wi, wd), (i, &(d, _))| {
            if d > wd {
                (i, d)
            } else {
                (wi, wd)
            }
        })
}

/// Squared Euclidean distance over quantized i16 vectors.
/// Fixed-size array lets LLVM auto-vectorize with AVX2.
#[inline(always)]
fn dist_sq(a: &[i16; STRIDE], b: &[i16; STRIDE]) -> i32 {
    let mut sum = 0i32;
    for i in 0..STRIDE {
        let d = a[i] as i32 - b[i] as i32;
        sum += d * d;
    }
    sum
}

pub fn quantize(v: &[f32; 14]) -> [i16; STRIDE] {
    let mut q = [0i16; STRIDE];
    for i in 0..14 {
        q[i] = match i {
            5 | 6 => (v[i].clamp(-1.0, 1.0) * SCALE).round() as i16,
            9 | 10 | 11 => {
                if v[i] > 0.5 {
                    SCALE as i16
                } else {
                    0
                }
            }
            _ => (v[i].clamp(0.0, 1.0) * SCALE + 0.5) as i16,
        };
    }
    q
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantize_clamps_and_scales() {
        let v = [0.5f32; 14];
        let q = quantize(&v);
        // Continuous: (0.5 * 8192 + 0.5) as i16 = 4096
        assert_eq!(q[0], 4096);
        // Dims 5-6: (0.5 * 8192).round() = 4096
        assert_eq!(q[5], 4096);
        // Binary dims 9-11: 0.5 is not > 0.5, so 0
        assert_eq!(q[9], 0);
        assert_eq!(q[10], 0);
        assert_eq!(q[11], 0);
        // Padding
        assert_eq!(q[14], 0);
        assert_eq!(q[15], 0);
    }

    #[test]
    fn quantize_binary_dims() {
        let mut v = [0.0f32; 14];
        v[9] = 1.0;
        v[10] = 0.0;
        v[11] = 1.0;
        let q = quantize(&v);
        assert_eq!(q[9], 8192);
        assert_eq!(q[10], 0);
        assert_eq!(q[11], 8192);
    }

    #[test]
    fn quantize_negative_sentinel() {
        let mut v = [0.0f32; 14];
        v[5] = -1.0;
        v[6] = -1.0;
        let q = quantize(&v);
        assert_eq!(q[5], -8192);
        assert_eq!(q[6], -8192);
    }

    #[test]
    fn dist_sq_identical() {
        let a = [100i16; STRIDE];
        assert_eq!(dist_sq(&a, &a), 0);
    }

    #[test]
    fn dist_sq_known() {
        let a = [0i16; STRIDE];
        let mut b = [0i16; STRIDE];
        b[0] = 3;
        b[1] = 4;
        assert_eq!(dist_sq(&a, &b), 25);
    }

    #[test]
    fn search_returns_valid_score() {
        let index = FraudIndex::new();
        let q = quantize(&[0.5f32; 14]);
        let score = index.search(&q);
        assert!(score >= 0.0 && score <= 1.0);
        // Score must be one of 0.0, 0.2, 0.4, 0.6, 0.8, 1.0
        let valid = [0.0, 0.2, 0.4, 0.6, 0.8, 1.0];
        assert!(valid.iter().any(|&v| (score - v).abs() < 1e-6));
    }
}
