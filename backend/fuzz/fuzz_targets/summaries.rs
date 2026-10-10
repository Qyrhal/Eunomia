//! The pure summary functions that digest third-party JSON (finance, week,
//! recordings). Input is split on newlines into up to three JSON values.
#![no_main]
use eunomia_backend::connectors::clients::{compute_finance_summary, compute_pocketai_summary, compute_week_summary};
use libfuzzer_sys::fuzz_target;
use serde_json::Value;

fuzz_target!(|data: &[u8]| {
    let mut parts = data.split(|b| *b == b'\n').map(|p| serde_json::from_slice::<Value>(p).unwrap_or(Value::Null));
    let (a, b, c) = (
        parts.next().unwrap_or(Value::Null),
        parts.next().unwrap_or(Value::Null),
        parts.next().unwrap_or(Value::Null),
    );
    let _ = compute_finance_summary(&a, &b, &c);
    let _ = compute_week_summary(&a);
    let _ = compute_pocketai_summary(&a);
});
