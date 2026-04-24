use axum::{
    extract::{Json, State},
    http::StatusCode,
};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use crate::index::FraudIndex;
use crate::types::{FraudRequest, FraudResponse, NormConsts};
use crate::vectorizer::vectorize;

#[derive(Clone)]
pub struct AppState {
    pub index: Arc<FraudIndex>,
    pub ready: Arc<AtomicBool>,
    pub norm: Arc<NormConsts>,
    pub mcc_risk: Arc<HashMap<String, f32>>,
}

pub async fn ready(State(s): State<AppState>) -> StatusCode {
    if s.ready.load(Ordering::Acquire) {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

const FRAUD_THRESHOLD: f32 = 0.6;

pub async fn fraud_score(
    State(s): State<AppState>,
    Json(req): Json<FraudRequest>,
) -> Json<FraudResponse> {
    let vector = vectorize(&req, &s.norm, &s.mcc_risk);
    let fraud_score = s.index.search(&vector);
    Json(FraudResponse {
        approved: fraud_score < FRAUD_THRESHOLD,
        fraud_score,
    })
}
