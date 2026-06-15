//! Phase 1.5 - handwriting proxy: pure payload-builder + response-parser tests (no network).

use kogu::recognize::{build_payload, parse_candidates};
use serde_json::json;

fn stroke(points: &[(f64, f64, f64)]) -> Vec<[f64; 3]> {
    points.iter().map(|&(x, y, t)| [x, y, t]).collect()
}

// 1. A single stroke is encoded as Google's [[xs],[ys],[ts]] triple.
#[test]
fn payload_single_stroke_shape() {
    let strokes = vec![stroke(&[(1.0, 2.0, 0.0), (3.0, 4.0, 10.0)])];
    let p = build_payload(300.0, 300.0, &strokes, "zh");
    let ink = &p["requests"][0]["ink"];
    assert_eq!(ink[0][0], json!([1.0, 3.0])); // xs
    assert_eq!(ink[0][1], json!([2.0, 4.0])); // ys
    assert_eq!(ink[0][2], json!([0.0, 10.0])); // ts
    assert_eq!(p["requests"][0]["writing_guide"]["writing_area_width"], 300.0);
}

// 2. Multiple strokes are preserved in order and count.
#[test]
fn payload_multi_stroke() {
    let strokes = vec![stroke(&[(0.0, 0.0, 0.0)]), stroke(&[(5.0, 5.0, 1.0)]), stroke(&[(9.0, 9.0, 2.0)])];
    let p = build_payload(256.0, 256.0, &strokes, "zh");
    assert_eq!(p["requests"][0]["ink"].as_array().unwrap().len(), 3);
}

// 3. Empty strokes still produce a valid (empty-ink) payload.
#[test]
fn payload_empty_ink() {
    let p = build_payload(100.0, 100.0, &[], "zh");
    assert_eq!(p["requests"][0]["ink"], json!([]));
}

// 4. Language tags map to Google's codes (edge: traditional + Cantonese).
#[test]
fn payload_language_mapping() {
    assert_eq!(build_payload(1.0, 1.0, &[], "zh")["requests"][0]["language"], "zh");
    assert_eq!(build_payload(1.0, 1.0, &[], "zh_TW")["requests"][0]["language"], "zh_TW");
    assert_eq!(build_payload(1.0, 1.0, &[], "ja")["requests"][0]["language"], "ja");
    assert_eq!(
        build_payload(1.0, 1.0, &[], "yue")["requests"][0]["language"],
        "yue-hant-t-i0-und"
    );
}

// 5. A SUCCESS response yields the ranked candidate list.
#[test]
fn parse_success() {
    let resp = json!(["SUCCESS", [["req0", ["木", "本", "末"], [], {}]]]);
    assert_eq!(parse_candidates(&resp), vec!["木", "本", "末"]);
}

// --- edge cases ---

// E1. A non-SUCCESS status yields no candidates (never panics).
#[test]
fn parse_failure_status() {
    assert!(parse_candidates(&json!(["FAILED_TO_PARSE", []])).is_empty());
}

// E2. Malformed / unexpected shapes yield empty, not a panic.
#[test]
fn parse_malformed() {
    assert!(parse_candidates(&json!(null)).is_empty());
    assert!(parse_candidates(&json!(["SUCCESS"])).is_empty());
    assert!(parse_candidates(&json!(["SUCCESS", [["id"]]])).is_empty());
    assert!(parse_candidates(&json!({"unexpected": true})).is_empty());
}

// E3. SUCCESS with an empty candidate array is handled.
#[test]
fn parse_empty_candidates() {
    assert!(parse_candidates(&json!(["SUCCESS", [["id", [], [], {}]]])).is_empty());
}
