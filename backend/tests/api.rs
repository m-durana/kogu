//! Phase 1.3-1.4 API regression probes (DESIGN.md §6.2) + edge cases.
//! Runs the real router (oneshot) against the built data/kanzi.sqlite.
//! Run from the backend/ dir: `cargo test`.

use std::sync::OnceLock;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use kanzi::{build_router, state::AppState};

fn state() -> AppState {
    static S: OnceLock<AppState> = OnceLock::new();
    S.get_or_init(|| {
        let path = std::env::var("KANZI_DB").unwrap_or_else(|_| "../data/kanzi.sqlite".into());
        AppState::load(&path).expect("load DB (run the pipeline build first)")
    })
    .clone()
}

fn enc(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => o.push(b as char),
            _ => o.push_str(&format!("%{:02X}", b)),
        }
    }
    o
}

async fn get(uri: &str) -> (StatusCode, Value) {
    let app = build_router(state());
    let resp = app.oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap()).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let val = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, val)
}

async fn search(q: &str) -> Value {
    get(&format!("/search?q={}", enc(q))).await.1
}

fn headwords(v: &Value) -> Vec<String> {
    v["results"].as_array().unwrap().iter().map(|r| r["headword"].as_str().unwrap().to_string()).collect()
}
fn varieties(v: &Value) -> Vec<String> {
    v["results"].as_array().unwrap().iter().map(|r| r["variety"].as_str().unwrap().to_string()).collect()
}

#[tokio::test]
async fn health_ok() {
    let (st, v) = get("/health").await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["status"], "ok");
}

// 1. exact Han lookup, sensibly ranked, with the right gloss.
#[tokio::test]
async fn exact_han() {
    let v = search("機場").await;
    assert_eq!(v["classified_as"], "han");
    let top = &v["results"][0];
    assert_eq!(top["headword"], "機場");
    assert_eq!(top["match_type"], "exact");
    assert!(top["glosses"][0].as_str().unwrap().to_lowercase().contains("airport"));
}

// 2. cross-script: the simplified/Japanese form 学校 finds BOTH the trad zh lexeme and the ja one.
#[tokio::test]
async fn cross_script_expansion() {
    let v = search("学校").await;
    let hw = headwords(&v);
    let var = varieties(&v);
    assert!(hw.iter().zip(&var).any(|(h, vv)| h == "學校" && vv == "zh"), "missing zh 學校");
    assert!(hw.iter().zip(&var).any(|(h, vv)| h == "学校" && vv == "ja"), "missing ja 学校");
}

// 3. keep-vs-convert: 缶 is returned as itself, not silently converted away.
#[tokio::test]
async fn keep_not_convert() {
    assert!(headwords(&search("缶").await).contains(&"缶".to_string()));
    assert!(headwords(&search("糸").await).contains(&"糸".to_string()));
}

// 4. English pivot (gloss FTS) surfaces both languages' airport words.
#[tokio::test]
async fn english_pivot() {
    let v = search("airport").await;
    assert_eq!(v["classified_as"], "latin");
    let hw = headwords(&v);
    assert!(hw.contains(&"空港".to_string()), "missing ja 空港");
    assert!(hw.contains(&"機場".to_string()), "missing zh 機場");
}

// 4b. English results are ranked by relevance: an exact "airport" word ranks at the very top,
//     not an incidental match like デッキ (deck). Regression for the bad-ranking report.
#[tokio::test]
async fn english_ranked_by_relevance() {
    let v = search("airport").await;
    let hw = headwords(&v);
    let top = &hw[0];
    // top must be a word that *is* airport/airfield, not an incidental compound
    let exact = ["空港", "機場", "飛行場", "飛機場"];
    assert!(exact.contains(&top.as_str()), "top airport result was {top:?}, expected an exact airport word");
    // an exact-gloss airport word must outrank デッキ ("deck (of a ship)")
    let pos = |w: &str| hw.iter().position(|h| h == w);
    if let (Some(a), Some(d)) = (pos("空港"), pos("デッキ")) {
        assert!(a < d, "空港 (#{a}) should rank above デッキ (#{d})");
    }
}

// 5. toneless pinyin keeps many candidates instead of bailing.
#[tokio::test]
async fn toneless_pinyin_multi() {
    let v = search("xue").await;
    assert!(v["results"].as_array().unwrap().len() > 5);
    // ren (the doc's probe) likewise must not bail
    assert!(search("ren").await["results"].as_array().unwrap().len() > 5);
}

// --- edge cases ---

// E1. 会社 false-friend material: the Japanese 会社 (company) and a Chinese 會社 both surface,
//     in different varieties — the raw material for the Phase-2 false-friend label.
#[tokio::test]
async fn false_friend_material() {
    let v = search("会社").await;
    let pairs: Vec<(String, String)> = v["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["variety"].as_str().unwrap().into(), r["headword"].as_str().unwrap().into()))
        .collect();
    assert!(pairs.contains(&("ja".into(), "会社".into())), "missing ja 会社");
    assert!(pairs.iter().any(|(vv, _)| vv == "zh"), "missing a zh counterpart");
}

// E2. 自転車 (ja bicycle) must NOT drag in a spurious Chinese false synonym.
#[tokio::test]
async fn no_spurious_cross_hit() {
    let v = search("自転車").await;
    let var = varieties(&v);
    assert!(var.iter().all(|vv| vv == "ja"), "spurious non-ja hit for 自転車: {var:?}");
}

// E3. 夾 must not drag in 袷 (semantic-variant over-fire guard: semantic edges never expand).
#[tokio::test]
async fn no_semantic_overfire() {
    let hw = headwords(&search("夾").await);
    assert!(!hw.iter().any(|h| h.contains('袷')), "夾 over-fired into 袷: {hw:?}");
}

// E4. English search is order-independent / predictable: 'train station' == 'station train'.
#[tokio::test]
async fn english_order_independent() {
    let mut a = headwords(&search("train station").await);
    let mut b = headwords(&search("station train").await);
    a.sort();
    b.sort();
    assert_eq!(a, b);
}

// --- Phase 2: concept layer / translation / relation labels ---

async fn translate(q: &str) -> Value {
    get(&format!("/translate?q={}", enc(q))).await.1
}

fn entry_of(results: &Value, variety: &str, headword: &str) -> Value {
    results["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["variety"] == variety && r["headword"] == headword)
        .expect("result present")
        .clone()
}

// P1. English pivot: /translate groups equivalents across systems under a concept.
#[tokio::test]
async fn translate_airport_groups_systems() {
    let v = translate("airport").await;
    let group = v["concepts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["concept"] == "airport")
        .expect("airport concept");
    let members: Vec<(String, String)> = group["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| (m["variety"].as_str().unwrap().into(), m["headword"].as_str().unwrap().into()))
        .collect();
    assert!(members.contains(&("ja".into(), "空港".into())), "missing ja 空港");
    assert!(members.contains(&("zh".into(), "機場".into())), "missing zh 機場");
}

// P2. cognate: 學校(zh) and 学校(ja) share the school concept -> labelled cognate.
#[tokio::test]
async fn cognate_label() {
    let hit = entry_of(&search("學校").await, "zh", "學校");
    let id = hit["lexeme_id"].as_i64().unwrap();
    let e = get(&format!("/entry/{id}")).await.1;
    let sf = &e["same_form"].as_array().unwrap()[0];
    assert_eq!(sf["headword"], "学校");
    assert_eq!(sf["relation"], "cognate");
}

// P3. false friend: 手紙 is zh "toilet paper" / ja "letter" -> same form, no shared concept.
#[tokio::test]
async fn false_friend_label() {
    let hit = entry_of(&search("手紙").await, "zh", "手紙");
    let id = hit["lexeme_id"].as_i64().unwrap();
    let e = get(&format!("/entry/{id}")).await.1;
    let ja = e["same_form"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["variety"] == "ja" && l["headword"] == "手紙")
        .expect("ja 手紙 in same_form");
    assert_eq!(ja["relation"], "false-friend");
}

// P4. 同義: an entry surfaces same-meaning different-word equivalents (incl. cross-language).
#[tokio::test]
async fn translations_same_meaning() {
    let hit = entry_of(&search("手紙").await, "zh", "手紙");
    let id = hit["lexeme_id"].as_i64().unwrap();
    let e = get(&format!("/entry/{id}")).await.1;
    let trans = e["translations"].as_array().unwrap();
    assert!(!trans.is_empty(), "expected 同義 translations for 手紙");
    // every translation carries a concept label and is not the anchor word itself
    assert!(trans.iter().all(|t| t["concept"].is_string()));
    assert!(trans.iter().any(|t| t["variety"] == "ja"), "expected a cross-language synonym");
}

// P6. orthographic "why": 学 carries reform-tagged edges to orthodox 學 (simp + shinjitai),
//     and readings across varieties are present (phonological why).
#[tokio::test]
async fn why_orthographic_and_phonological() {
    let hit = entry_of(&search("学校").await, "ja", "学校");
    let id = hit["lexeme_id"].as_i64().unwrap();
    let w = get(&format!("/why/{id}")).await.1;
    let gaku = w["characters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["ch"] == "学")
        .expect("学 in why");
    // orthographic: edges to 學 labelled with reforms
    let parents: Vec<String> =
        gaku["variants"].as_array().unwrap().iter().map(|v| v["parent"].as_str().unwrap().into()).collect();
    assert!(parents.contains(&"學".to_string()), "学 should chain to 學");
    let types: Vec<String> =
        gaku["variants"].as_array().unwrap().iter().map(|v| v["edge_type"].as_str().unwrap().into()).collect();
    assert!(types.iter().any(|t| t == "simplification") && types.iter().any(|t| t == "shinjitai"));
    assert!(gaku["variants"].as_array().unwrap().iter().any(|v| v["reform_name"].is_string()));
    // phonological: 學 has readings across varieties
    let xue = entry_of(&search("學校").await, "zh", "學校");
    let xid = xue["lexeme_id"].as_i64().unwrap();
    let we = get(&format!("/why/{xid}")).await.1;
    let c = we["characters"].as_array().unwrap().iter().find(|c| c["ch"] == "學").unwrap();
    let kinds: Vec<String> =
        c["readings"].as_array().unwrap().iter().map(|r| r["kind"].as_str().unwrap().into()).collect();
    assert!(kinds.iter().any(|k| k == "pinyin") && kinds.iter().any(|k| k == "jyutping") && kinds.iter().any(|k| k == "onyomi"));
}

// P7 (edge). /why for unknown id is 404.
#[tokio::test]
async fn why_unknown_404() {
    let (st, _) = get("/why/99999999").await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

// P5 (edge). a nonsense English term yields no concepts (never errors).
#[tokio::test]
async fn translate_unknown_empty() {
    let v = translate("zzzznotaword").await;
    assert!(v["concepts"].as_array().unwrap().is_empty());
}

// --- Phase 3.3: Cantonese ---

// C1. Cantonese colloquial words / 粵字 exist as first-class yue lexemes.
#[tokio::test]
async fn cantonese_lexemes() {
    let hw = headwords(&search("唔該").await);
    assert!(hw.contains(&"唔該".to_string()), "missing Cantonese 唔該");
    let var = varieties(&search("唔該").await);
    assert!(var.iter().any(|v| v == "yue"));
}

// C2. jyutping search works (the original rejected it): toneless jyutping finds the word.
#[tokio::test]
async fn jyutping_search() {
    // 唔該 = m4 goi1 -> toneless "mgoi"
    let hw = headwords(&search("mgoi").await);
    assert!(hw.contains(&"唔該".to_string()), "jyutping 'mgoi' should find 唔該");
}

// C3. shared vocab carries a jyutping reading (學校 = hok6 haau6).
#[tokio::test]
async fn shared_vocab_has_jyutping() {
    let hit = entry_of(&search("學校").await, "zh", "學校");
    let id = hit["lexeme_id"].as_i64().unwrap();
    let e = get(&format!("/entry/{id}")).await.1;
    let has = e["readings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["kind"] == "jyutping" && r["value"].as_str().unwrap().contains("hok6"));
    assert!(has, "學校 should carry a jyutping reading");
}

// E5. entry endpoint returns full structure; unknown id is 404.
#[tokio::test]
async fn entry_and_404() {
    let v = search("學校").await;
    // pick the zh lexeme specifically (ja 学校 may rank first by frequency)
    let id = v["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["variety"] == "zh" && r["headword"] == "學校")
        .expect("zh 學校 in results")["lexeme_id"]
        .as_i64()
        .unwrap();
    let (st, e) = get(&format!("/entry/{id}")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(e["headword"], "學校");
    assert!(e["characters"].as_array().unwrap().len() >= 2);
    let (st404, _) = get("/entry/99999999").await;
    assert_eq!(st404, StatusCode::NOT_FOUND);
}
