mod handler;
mod index;
mod types;
mod vectorizer;

use axum::routing::{get, post};
use axum::Router;
use handler::AppState;
use index::FraudIndex;
use std::collections::HashMap;
use std::sync::{atomic::AtomicBool, Arc};
use tokio::net::TcpListener;
use types::NormConsts;

#[tokio::main(flavor = "multi_thread", worker_threads = 1)]
async fn main() {
    let gz = include_bytes!("../resources/references.json.gz");
    let mcc_raw = include_bytes!("../resources/mcc_risk.json");
    let norm_raw = include_bytes!("../resources/normalization.json");

    let index = Arc::new(FraudIndex::build(gz));
    let mcc_risk = Arc::new(
        serde_json::from_slice::<HashMap<String, f32>>(mcc_raw)
            .expect("mcc_risk.json embedded in binary is invalid"),
    );
    let norm = Arc::new(
        serde_json::from_slice::<NormConsts>(norm_raw)
            .expect("normalization.json embedded in binary is invalid"),
    );
    let ready = Arc::new(AtomicBool::new(true));

    let state = AppState {
        index,
        ready,
        norm,
        mcc_risk,
    };

    let app = Router::new()
        .route("/ready", get(handler::ready))
        .route("/fraud-score", post(handler::fraud_score))
        .with_state(state);

    let listener = TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
