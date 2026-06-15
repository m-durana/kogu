//! Phase 1.3-1.4 API regression probes (DESIGN.md §6.2) + edge cases.
//! Runs the real router (oneshot) against the built data/kogu.sqlite.
//! Run from the backend/ dir: `cargo test`.

use std::sync::OnceLock;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use kogu::{build_router, state::AppState};

fn state() -> AppState {
    static S: OnceLock<AppState> = OnceLock::new();
    S.get_or_init(|| {
        // the API tests don't exercise OCR; loading the ONNX runtime without ORT_DYLIB_PATH blocks.
        std::env::set_var("KOGU_SKIP_OCR", "1");
        let path = std::env::var("KOGU_DB").unwrap_or_else(|_| "../data/kogu.sqlite".into());
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

async fn post(uri: &str, body: Vec<u8>) -> StatusCode {
    let app = build_router(state());
    app.oneshot(Request::builder().method("POST").uri(uri).body(Body::from(body)).unwrap())
        .await
        .unwrap()
        .status()
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

// --- OCR endpoint wiring (the char-split logic itself is unit-tested in src/ocr.rs) ---

// OCR1. /ocr is POST-only and wired (GET is 405, not 404).
#[tokio::test]
async fn ocr_is_post_only() {
    let (st, _) = get("/ocr").await;
    assert_eq!(st, StatusCode::METHOD_NOT_ALLOWED);
}

// OCR2. empty body is rejected cleanly: 400 (engine present) or 503 (models absent) — never 500/404.
#[tokio::test]
async fn ocr_empty_body_handled() {
    let st = post("/ocr", Vec::new()).await;
    assert!(
        st == StatusCode::BAD_REQUEST || st == StatusCode::SERVICE_UNAVAILABLE,
        "unexpected status for empty /ocr: {st}"
    );
}

// OCR3. non-image bytes never 500: 400 bad_image (engine present) or 503 (absent).
#[tokio::test]
async fn ocr_garbage_not_500() {
    let st = post("/ocr", b"not an image".to_vec()).await;
    assert!(st == StatusCode::BAD_REQUEST || st == StatusCode::SERVICE_UNAVAILABLE, "got {st}");
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

// --- Phase 3.2: lexical origin badges + etymology passthrough ---

// O1. origin badge: Chinese 会社 is tagged borrowed-from-japanese (data-only, no LLM).
#[tokio::test]
async fn origin_badge() {
    // the zh lexeme's headword is 會社 (traditional); 会社 is its simplified form
    let v = search("会社").await;
    let hit = v["results"].as_array().unwrap().iter().find(|r| r["variety"] == "zh").expect("zh 会社");
    let id = hit["lexeme_id"].as_i64().unwrap();
    let e = get(&format!("/entry/{id}")).await.1;
    let badges: Vec<String> =
        e["origin_badges"].as_array().unwrap().iter().map(|b| b.as_str().unwrap().into()).collect();
    assert!(badges.iter().any(|b| b == "borrowed-from-japanese"), "badges were {badges:?}");
}

// O2. etymology passthrough: 空港 carries its Wiktionary etymology paragraph verbatim.
#[tokio::test]
async fn etymology_passthrough() {
    let hit = entry_of(&search("空港").await, "ja", "空港");
    let id = hit["lexeme_id"].as_i64().unwrap();
    let e = get(&format!("/entry/{id}")).await.1;
    let ety = e["etymology"].as_str().unwrap_or("");
    assert!(ety.contains('空') && ety.contains('港'), "etymology was {ety:?}");
}

// S1. mixed kanji+kana words (甘い, 食べる) classify as kana but must match by written form.
#[tokio::test]
async fn mixed_kanji_kana_word() {
    assert!(headwords(&search("甘い").await).contains(&"甘い".to_string()), "甘い not found");
    assert!(headwords(&search("食べる").await).contains(&"食べる".to_string()), "食べる not found");
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

// ─────────────────────────────────────────────────────────────────────────────
// Probe batteries — the 115-word and 100-edge-case sets, distilled into invariants
// that must hold on every build. (Run from backend/: `cargo test --release`.)
// ─────────────────────────────────────────────────────────────────────────────

/// search q, then fetch /entry of the top hit.
async fn entry_top(q: &str) -> Value {
    let v = search(q).await;
    let id = v["results"].as_array().unwrap().first().expect("a result")["lexeme_id"].as_i64().unwrap();
    get(&format!("/entry/{id}")).await.1
}

fn same_form_has_ff(e: &Value) -> bool {
    e["same_form"].as_array().unwrap().iter().any(|l| l["relation"] == "false-friend")
}

// B1. Nothing in the batteries comes back empty — incl. kokuji with no word-lexeme (込/凪/榊,
// resolved by the character-page fallback) and the keep-vs-convert own-chars (干/缶/糸).
#[tokio::test]
async fn probe_no_zero_hits() {
    let must_resolve = [
        "鬱", "薔薇", "麒麟", "干", "缶", "糸", "后", "里", "面", "发", "复", "系", "廣", "圖",
        "驛", "龍", "馬", "門", "夾", "戸", "龜", "峠", "込", "凪", "雫", "躾", "畑", "腺", "働",
        "榊", "唔", "係", "嘅", "喺", "咗", "冇", "嘢", "啲", "揾", "嚟", "行", "和", "重", "差",
        "己", "已", "巳", "未", "末", "龘", "手紙", "汽車", "会社", "自転車", "空港", "機場",
        "中国", "日本", "学校", "先生", "音楽",
    ];
    for q in must_resolve {
        let n = search(q).await["results"].as_array().unwrap().len();
        assert!(n >= 1, "{q} returned zero results");
    }
}

// B2. The L-section false friends are labelled false-friend on the looked-up entry.
#[tokio::test]
async fn probe_false_friends_flag() {
    for q in ["手紙", "汽車", "娘", "勉強", "大丈夫", "留守", "会社"] {
        let e = entry_top(q).await;
        assert!(same_form_has_ff(&e), "{q} should have a false-friend sibling");
    }
}

// B3. Clean cognates / same-word pairs must NOT carry a false-friend flag.
#[tokio::test]
async fn probe_cognate_controls() {
    for q in ["砂糖", "愛", "中国", "學校"] {
        let e = entry_top(q).await;
        assert!(!same_form_has_ff(&e), "{q} should not be flagged a false friend");
    }
}

// B4. Cross-language false friend whose forms are variant glyphs: 会社 (jp "company") and 會社
// (zh "guild") — 会 is the shinjitai of 會, so the variant-spelling cognate rule must NOT apply
// across languages (the canonical 会社 case from the probe set).
#[tokio::test]
async fn probe_kaisha_cross_language_false_friend() {
    let hit = entry_of(&search("会社").await, "ja", "会社");
    let id = hit["lexeme_id"].as_i64().unwrap();
    let e = get(&format!("/entry/{id}")).await.1;
    let zh = e["same_form"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["variety"] == "zh")
        .expect("zh 會社 in same_form");
    assert_eq!(zh["relation"], "false-friend", "会社 jp/zh should be a false friend");
}

// B5. Romaji input resolves, tolerant of long-vowel / n-m spelling.
#[tokio::test]
async fn probe_romaji_input() {
    for (q, want) in [("tokyo", "東京"), ("toukyou", "東京"), ("shinbun", "新聞"), ("shimbun", "新聞"), ("sensei", "先生")] {
        let hw = headwords(&search(q).await);
        assert!(hw.iter().any(|h| h == want), "{q} should surface {want}, got {hw:?}");
    }
}

// B6. Phonetic queries rank the common word first (frequency data, not arbitrary ties).
#[tokio::test]
async fn probe_phonetic_ranking() {
    for (q, top) in [("ren", "人"), ("nv3", "女"), ("dian4hua4", "電話"), ("diànhuà", "電話")] {
        let hw = headwords(&search(q).await);
        assert_eq!(hw.first().map(String::as_str), Some(top), "{q} top should be {top}, got {hw:?}");
    }
}

// B7. Japanese on/kun char readings are kana — never romaji junk like "K0"/"ABURA".
#[tokio::test]
async fn probe_no_romaji_onkun_junk() {
    for q in ["愛", "水", "人", "込", "畑", "学校"] {
        let e = entry_top(q).await;
        for c in e["characters"].as_array().unwrap() {
            for r in c["readings"].as_array().unwrap() {
                let kind = r["kind"].as_str().unwrap();
                if kind == "onyomi" || kind == "kunyomi" {
                    let v = r["value"].as_str().unwrap();
                    assert!(!v.bytes().any(|b| b.is_ascii_alphanumeric()), "{q}: romaji on/kun {v:?}");
                }
            }
        }
    }
}

// B8. A kokuji with no word-lexeme resolves to a character page (negative id) with its 熟語.
#[tokio::test]
async fn probe_kokuji_fallback() {
    let v = search("込").await;
    let top = &v["results"].as_array().unwrap()[0];
    assert!(top["lexeme_id"].as_i64().unwrap() < 0, "込 should use a synthetic character id");
    let id = top["lexeme_id"].as_i64().unwrap();
    let e = get(&format!("/entry/{id}")).await.1;
    assert!(!e["compounds"].as_array().unwrap().is_empty(), "込 should list 熟語 compounds");
}

// B9. A single character lists the words that contain it.
#[tokio::test]
async fn probe_single_char_compounds() {
    let e = entry_top("愛").await;
    assert!(e["compounds"].as_array().unwrap().len() >= 5, "愛 should list compounds");
}

// B10. SQL/FTS wildcards & injection don't crash or match-everything.
#[tokio::test]
async fn probe_wildcard_injection_safe() {
    for q in ["'", "%", "_", "*", "' OR 1=1 --"] {
        let (st, v) = get(&format!("/search?q={}", enc(q))).await;
        assert_eq!(st, StatusCode::OK, "{q} should not error");
        let n = v["results"].as_array().map(|a| a.len()).unwrap_or(0);
        assert!(n < 50, "{q} should not wildcard-match everything (got {n})");
    }
}
