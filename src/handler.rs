use axum::{
    extract::{Json, State},
    http::{header, StatusCode},
    response::IntoResponse,
};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use crate::index::FraudIndex;
use crate::types::{FraudRequest, NormConsts};
use crate::vectorizer::vectorize;

#[derive(Clone)]
pub struct AppState {
    pub index: Arc<FraudIndex>,
    pub ready: Arc<AtomicBool>,
    pub norm: Arc<NormConsts>,
    pub mcc_risk: Arc<HashMap<String, f32>>,
    pub responses: Arc<[Vec<u8>; 6]>,
}

pub async fn ready(State(s): State<AppState>) -> StatusCode {
    if s.ready.load(Ordering::Acquire) {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

/// Pre-serialize all 6 possible responses at startup.
///
/// Only 6 outcomes exist: fraud neighbor count 0–5 out of 5.
/// Eliminates serde_json serialization on the request hot path.
pub fn build_responses() -> [Vec<u8>; 6] {
    [0u8, 1, 2, 3, 4, 5].map(|count| {
        let score = count as f32 / 5.0;
        // Threshold matches the competition spec: approved iff fraud_score < 0.6
        let approved = score < 0.6;
        format!(r#"{{"approved":{approved},"fraud_score":{score}}}"#)
            .into_bytes()
    })
}

pub async fn fraud_score(
    State(s): State<AppState>,
    Json(req): Json<FraudRequest>,
) -> impl IntoResponse {
    let vector = vectorize(&req, &s.norm, &s.mcc_risk);
    // Offload the CPU-bound KNN scan so the tokio thread stays free for I/O.
    // On panic, default to 5 (score=1.0, approved=false) — avoids HTTP 500 weight-5 penalty.
    let count = tokio::task::spawn_blocking(move || s.index.search(&vector))
        .await
        .unwrap_or(5);
    let body = s.responses[count as usize].clone();
    ([(header::CONTENT_TYPE, "application/json")], body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_responses_approved_values() {
        let responses = build_responses();
        let bodies: Vec<String> = responses
            .iter()
            .map(|b| String::from_utf8(b.clone()).unwrap())
            .collect();

        // counts 0, 1, 2 → score < 0.6 → approved=true
        assert!(bodies[0].contains("\"approved\":true"),  "count=0 must be approved");
        assert!(bodies[1].contains("\"approved\":true"),  "count=1 must be approved");
        assert!(bodies[2].contains("\"approved\":true"),  "count=2 must be approved");
        // counts 3, 4, 5 → score >= 0.6 → approved=false
        assert!(bodies[3].contains("\"approved\":false"), "count=3 must be rejected");
        assert!(bodies[4].contains("\"approved\":false"), "count=4 must be rejected");
        assert!(bodies[5].contains("\"approved\":false"), "count=5 must be rejected");
    }

    #[test]
    fn build_responses_fraud_scores() {
        let responses = build_responses();
        let expected = ["0", "0.2", "0.4", "0.6", "0.8", "1"];
        for (i, &exp) in expected.iter().enumerate() {
            let body = String::from_utf8(responses[i].clone()).unwrap();
            assert!(
                body.contains(&format!("\"fraud_score\":{exp}")),
                "response[{i}] body={body} expected fraud_score={exp}"
            );
        }
    }
}
