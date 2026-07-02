//! OpenAPI spec sanity: the generated document (the one dump_openapi ships as
//! frontend/public/api-docs/openapi.json) stays parseable, complete and versioned.

use serde_json::Value;
use utoipa::OpenApi;

use kogu::openapi::ApiDoc;

fn spec() -> Value {
    let json = ApiDoc::openapi().to_pretty_json().expect("spec serializes");
    serde_json::from_str(&json).expect("spec is valid JSON")
}

#[test]
fn spec_parses_with_title_and_paths() {
    let s = spec();
    assert_eq!(s["info"]["title"], "Kogu API");
    assert!(s["paths"].is_object());
    assert!(s["components"]["schemas"].is_object());
}

#[test]
fn all_public_paths_documented() {
    let s = spec();
    let paths = s["paths"].as_object().unwrap();
    let expected = [
        "/health",
        "/search",
        "/suggest",
        "/entry/{id}",
        "/why/{id}",
        "/translate",
        "/segment",
        "/mt",
        "/recognize",
        "/ocr",
        "/tts/ja",
        "/clip/{variety}/{file}",
    ];
    for p in expected {
        assert!(paths.contains_key(p), "path {p} missing from the spec");
    }
    assert_eq!(paths.len(), expected.len(), "unexpected extra paths in the spec");
}

#[test]
fn server_url_is_public_api_base() {
    let s = spec();
    assert_eq!(s["servers"][0]["url"], "https://kogu.miro.build/api");
}

#[test]
fn hit_schema_has_jyut_field() {
    let s = spec();
    let hit = &s["components"]["schemas"]["Hit"]["properties"];
    assert!(hit["jyut"].is_object(), "Hit.jyut missing: {hit}");
    // the always-present core fields ride along
    for f in ["lexeme_id", "variety", "headword", "glosses", "match_type", "score"] {
        assert!(hit[f].is_object(), "Hit.{f} missing");
    }
}

#[test]
fn version_matches_crate() {
    let s = spec();
    assert_eq!(s["info"]["version"], env!("CARGO_PKG_VERSION"));
}
