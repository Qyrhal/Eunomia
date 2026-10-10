//! Every connector's `Source::map` turns third-party JSON into a cache record.
//! First input byte picks the source, the rest is the raw JSON. Must not panic,
//! and a mapped record must be an object with an `id`.
#![no_main]
use eunomia_backend::sources::registry;
use libfuzzer_sys::fuzz_target;
use serde_json::Value;

fuzz_target!(|data: &[u8]| {
    let Some((first, rest)) = data.split_first() else { return };
    let sources = registry::all();
    let src = &sources[*first as usize % sources.len()];
    let Ok(raw) = serde_json::from_slice::<Value>(rest) else { return };
    if let Some(rec) = src.map(&raw) {
        assert!(rec.get("id").is_some(), "{} mapped a record without id", src.key());
    }
});
