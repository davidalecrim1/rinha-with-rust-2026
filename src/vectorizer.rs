use chrono::{Datelike, Timelike};
use std::collections::HashMap;

use crate::types::{FraudRequest, NormConsts};

pub fn vectorize(req: &FraudRequest, c: &NormConsts, mcc_risk: &HashMap<String, f32>) -> [f32; 14] {
    let t_now = req.transaction.requested_at;

    let hour = t_now.hour() as f64;
    let dow = t_now.weekday().num_days_from_monday() as f64;

    let (minutes_since, km_from_last) = match &req.last_transaction {
        None => (-1.0f32, -1.0f32),
        Some(lt) => {
            let delta_minutes = (t_now - lt.timestamp).num_seconds().abs() as f64 / 60.0;
            (
                (delta_minutes / c.max_minutes) as f32,
                (lt.km_from_current / c.max_km) as f32,
            )
        }
    };

    let merchant = if req.customer.known_merchants.contains(&req.merchant.id) {
        0.0f32
    } else {
        1.0
    };

    let mcc_risk_val = mcc_risk.get(&req.merchant.mcc).copied().unwrap_or(0.5);

    [
        clamp(req.transaction.amount / c.max_amount),
        clamp(req.transaction.installments as f64 / c.max_installments),
        clamp((req.transaction.amount / req.customer.avg_amount) / c.amount_vs_avg_ratio),
        (hour / 23.0) as f32,
        (dow / 6.0) as f32,
        minutes_since.clamp(-1.0, 1.0),
        km_from_last.clamp(-1.0, 1.0),
        clamp(req.terminal.km_from_home / c.max_km),
        clamp(req.customer.tx_count_24h as f64 / c.max_tx_count_24h),
        req.terminal.is_online as u8 as f32,
        req.terminal.card_present as u8 as f32,
        merchant,
        mcc_risk_val,
        clamp(req.merchant.avg_amount / c.max_merchant_avg_amount),
    ]
}

#[inline(always)]
fn clamp(x: f64) -> f32 {
    (x as f32).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;
    use crate::types::{Customer, LastTransaction, Merchant, Terminal, Transaction};

    fn dt(s: &str) -> DateTime<chrono::FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    fn norm() -> NormConsts {
        NormConsts {
            max_amount: 10000.0,
            max_installments: 12.0,
            amount_vs_avg_ratio: 10.0,
            max_minutes: 1440.0,
            max_km: 1000.0,
            max_tx_count_24h: 20.0,
            max_merchant_avg_amount: 10000.0,
        }
    }

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    // Known vectors from DETECTION_RULES.md examples
    #[test]
    fn vectorize_legit_example() {
        let mut mcc_risk = HashMap::new();
        mcc_risk.insert("5411".to_string(), 0.15f32);

        let req = FraudRequest {
            id: "tx-1329056812".to_string(),
            transaction: Transaction {
                amount: 41.12,
                installments: 2,
                requested_at: dt("2026-03-11T18:45:53Z"),
            },
            customer: Customer {
                avg_amount: 82.24,
                tx_count_24h: 3,
                known_merchants: vec!["MERC-003".to_string(), "MERC-016".to_string()],
            },
            merchant: Merchant {
                id: "MERC-016".to_string(),
                mcc: "5411".to_string(),
                avg_amount: 60.25,
            },
            terminal: Terminal {
                is_online: false,
                card_present: true,
                km_from_home: 29.23,
            },
            last_transaction: None,
        };

        let v = vectorize(&req, &norm(), &mcc_risk);

        // [amount, installments, amount_vs_avg, hour, dow, minutes_since, km_last, km_home, tx_count, online, card, unknown_merchant, mcc_risk, merchant_avg]
        let expected: [f32; 14] = [
            0.0041, 0.1667, 0.05, 0.7826, 0.3333, -1.0, -1.0, 0.0292, 0.15, 0.0, 1.0, 0.0,
            0.15, 0.006,
        ];

        for (i, (&got, &exp)) in v.iter().zip(expected.iter()).enumerate() {
            assert!(
                approx_eq(got, exp),
                "dim {i}: got {got:.4} expected {exp:.4}"
            );
        }
    }

    #[test]
    fn vectorize_fraud_example() {
        let mut mcc_risk = HashMap::new();
        mcc_risk.insert("7802".to_string(), 0.75f32);

        let req = FraudRequest {
            id: "tx-3330991687".to_string(),
            transaction: Transaction {
                amount: 9505.97,
                installments: 10,
                requested_at: dt("2026-03-14T05:15:12Z"),
            },
            customer: Customer {
                avg_amount: 81.28,
                tx_count_24h: 20,
                known_merchants: vec![
                    "MERC-008".to_string(),
                    "MERC-007".to_string(),
                    "MERC-005".to_string(),
                ],
            },
            merchant: Merchant {
                id: "MERC-068".to_string(),
                mcc: "7802".to_string(),
                avg_amount: 54.86,
            },
            terminal: Terminal {
                is_online: false,
                card_present: true,
                km_from_home: 952.27,
            },
            last_transaction: None,
        };

        let v = vectorize(&req, &norm(), &mcc_risk);

        let expected: [f32; 14] = [
            0.9506, 0.8333, 1.0, 0.2174, 0.8333, -1.0, -1.0, 0.9523, 1.0, 0.0, 1.0, 1.0, 0.75,
            0.0055,
        ];

        for (i, (&got, &exp)) in v.iter().zip(expected.iter()).enumerate() {
            assert!(
                approx_eq(got, exp),
                "dim {i}: got {got:.4} expected {exp:.4}"
            );
        }
    }

    #[test]
    fn vectorize_with_last_transaction() {
        let mcc_risk = HashMap::new();
        let req = FraudRequest {
            id: "tx-test".to_string(),
            transaction: Transaction {
                amount: 100.0,
                installments: 1,
                requested_at: dt("2026-03-11T18:30:00Z"),
            },
            customer: Customer {
                avg_amount: 100.0,
                tx_count_24h: 1,
                known_merchants: vec![],
            },
            merchant: Merchant {
                id: "MERC-001".to_string(),
                mcc: "9999".to_string(),
                avg_amount: 100.0,
            },
            terminal: Terminal {
                is_online: true,
                card_present: false,
                km_from_home: 0.0,
            },
            last_transaction: Some(LastTransaction {
                timestamp: dt("2026-03-11T18:00:00Z"), // 30 min before
                km_from_current: 10.0,
            }),
        };

        let v = vectorize(&req, &norm(), &mcc_risk);

        // minutes_since: 30.0 / 1440.0 ≈ 0.02083
        assert!(approx_eq(v[5], 30.0 / 1440.0), "minutes_since: {}", v[5]);
        // km_from_last: 10.0 / 1000.0 = 0.01
        assert!(approx_eq(v[6], 0.01), "km_from_last: {}", v[6]);
        // is_online = true → 1.0
        assert_eq!(v[9], 1.0);
        // card_present = false → 0.0
        assert_eq!(v[10], 0.0);
        // unknown_merchant: MERC-001 not in [] → 1.0
        assert_eq!(v[11], 1.0);
        // mcc_risk default: 0.5
        assert_eq!(v[12], 0.5);
    }
}
