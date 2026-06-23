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

// 4c. An exact one-word sense outranks a fringe entry that merely mentions the word, even when the
//     fringe entry is far more frequent: "ear" → 耳, not 稲穂 ("ear of rice"). Was the headline bug.
#[tokio::test]
async fn english_exact_sense_beats_frequent_fringe() {
    let v = search("ear").await;
    let hw = headwords(&v);
    assert_eq!(hw.first().map(String::as_str), Some("耳"), "ear should lead with 耳, got {hw:?}");
    let pos = |w: &str| hw.iter().position(|h| h == w);
    if let (Some(e), Some(r)) = (pos("耳"), pos("稲穂")) {
        assert!(e < r, "耳 (#{e}) must outrank 稲穂 ear-of-rice (#{r})");
    }
}
// 4d. Plural query still finds the singular gloss (porter stemming) — used to return nothing.
#[tokio::test]
async fn english_plural_query_stems() {
    let hw = headwords(&search("ears").await);
    assert!(hw.contains(&"耳".to_string()), "ears should still find 耳, got {hw:?}");
}
#[tokio::test]
async fn english_plural_cats_stems() {
    let hw = headwords(&search("cats").await);
    assert!(hw.contains(&"猫".to_string()), "cats should find 猫, got {hw:?}");
}
#[tokio::test]
async fn english_plural_mountains_stems_and_ranks() {
    // porter stems mountains→mountain; 山 must surface near the top (it was absent before). It sits
    // just behind 岳 ("high mountain") only because the frequency data is saturated — a separate
    // freq-backfill concern; both are genuine mountain words, so top-3 is the honest bar here.
    let hw = headwords(&search("mountains").await);
    let rank = hw.iter().position(|h| h == "山");
    assert!(matches!(rank, Some(r) if r < 3), "山 should rank in the top 3 for mountains, got {hw:?}");
}
// 4e. Past-tense query finds the base gloss too (porter): "loved" → a love word.
#[tokio::test]
async fn english_past_tense_query_stems() {
    let hw = headwords(&search("loved").await);
    assert!(
        hw.iter().any(|h| ["愛", "愛する", "恋", "恋する", "愛す"].contains(&h.as_str())),
        "loved should find a love word, got {hw:?}"
    );
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
//     in different varieties - the raw material for the Phase-2 false-friend label.
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

// OCR2. empty body is rejected cleanly: 400 (engine present) or 503 (models absent) - never 500/404.
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
    // every translation is either a concept-pivot synonym (carries a concept label) or an explicit
    // equivalence edge (relation "equivalent"); never the anchor word itself.
    assert!(trans.iter().all(|t| t["concept"].is_string() || t["relation"] == "equivalent"));
    assert!(trans.iter().any(|t| t["variety"] == "ja"), "expected a cross-language synonym");
}

// P4b. explicit equivalence: colloquial Cantonese 冇 bridges to standard Chinese 沒有 (the precise
// CC-Canto "Mandarin equivalent" note, lifted into a structured lexeme_equivalent edge).
#[tokio::test]
async fn cantonese_equivalent_bridge() {
    let hit = entry_of(&search("冇").await, "yue", "冇");
    let id = hit["lexeme_id"].as_i64().unwrap();
    let e = get(&format!("/entry/{id}")).await.1;
    let trans = e["translations"].as_array().unwrap();
    assert!(
        trans
            .iter()
            .any(|t| t["relation"] == "equivalent" && t["variety"] == "zh" && t["headword"] == "沒有"),
        "expected 冇 → 中 沒有 equivalent bridge, got {trans:?}"
    );
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

// Structure: a character built from repetitions of one base is named (森 = three 木, resolved
// recursively through the doubled 林); a mixed-component character (好 = 女 + 子) is not.
#[tokio::test]
async fn char_decomposition_repeats() {
    for (word, base, count) in [("森", "木", 3), ("林", "木", 2)] {
        let hit = entry_of(&search(word).await, "zh", word);
        let id = hit["lexeme_id"].as_i64().unwrap();
        let e = get(&format!("/entry/{id}")).await.1;
        let c = &e["characters"][0];
        assert_eq!(c["decomp"]["base"].as_str(), Some(base), "{word} base");
        assert_eq!(c["decomp"]["count"].as_i64(), Some(count), "{word} count");
    }
    let hit = entry_of(&search("好").await, "zh", "好");
    let id = hit["lexeme_id"].as_i64().unwrap();
    let e = get(&format!("/entry/{id}")).await.1;
    assert!(e["characters"][0]["decomp"].is_null(), "好 (女+子) has no uniform decomposition");
}

// Kana search: as-you-type prefix, hiragana/katakana folding, and mixed kanji+kana prefix — the
// cases that previously returned an empty screen until the exact dictionary form was fully typed.
#[tokio::test]
async fn kana_reading_prefix() {
    // typing たべ should already surface たべる (food verbs), not nothing
    let hw = headwords(&search("たべ").await);
    assert!(hw.contains(&"食べる".to_string()), "たべ prefix should find 食べる, got {hw:?}");
}
#[tokio::test]
async fn kana_reading_prefix_gakko() {
    let hw = headwords(&search("がっこ").await);
    assert!(hw.contains(&"学校".to_string()), "がっこ prefix should find 学校, got {hw:?}");
}
#[tokio::test]
async fn kana_hiragana_to_katakana_fold() {
    // hiragana query for a katakana-stored loanword
    let hw = headwords(&search("てれび").await);
    assert!(hw.contains(&"テレビ".to_string()), "てれび should fold to テレビ, got {hw:?}");
}
#[tokio::test]
async fn kana_mixed_kanji_kana_prefix() {
    // 食べ (kanji + okurigana) is a prefix of 食べる / 食べ物
    let hw = headwords(&search("食べ").await);
    assert!(hw.contains(&"食べる".to_string()), "食べ prefix should find 食べる, got {hw:?}");
}
#[tokio::test]
async fn kana_exact_reading_still_works_and_leads() {
    // regression: the exact reading must still match and outrank prefix-only hits
    let v = search("たべる").await;
    let hw = headwords(&v);
    assert!(hw.contains(&"食べる".to_string()), "exact たべる should find 食べる");
    assert_eq!(hw.first().map(String::as_str), Some("食べる"), "exact reading should lead, got {hw:?}");
}
#[tokio::test]
async fn kana_full_written_form_still_works() {
    // regression: the fully-typed written form still resolves via surface_form exact
    let hw = headwords(&search("食べる").await);
    assert!(hw.contains(&"食べる".to_string()), "食べる exact form should resolve");
}

// Everyday-word: a single character that is a Japanese word points to the natural multi-character
// word Chinese actually writes for the same meaning (耳 → 耳朵). Derived, contained, freq-gated.
fn everyday_words(entry: &Value) -> Vec<String> {
    entry["translations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["relation"] == "everyday-word")
        .map(|l| l["headword"].as_str().unwrap().to_string())
        .collect()
}
#[tokio::test]
async fn everyday_word_ear() {
    // ja 耳 surfaces the Chinese everyday word 耳朵. It now arrives via a CURATED equivalence edge
    // (耳↔耳朵), which is a stronger link than the derived everyday-word and supersedes it (the
    // equivalence pass shares the dedup set and runs first). So accept either relation: what matters
    // is that the ja 耳 page shows 耳朵 in its bridge.
    let hit = entry_of(&search("耳").await, "ja", "耳");
    let e = get(&format!("/entry/{}", hit["lexeme_id"].as_i64().unwrap())).await.1;
    let shows_duo = e["translations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|l| l["headword"] == "耳朵" && matches!(l["relation"].as_str(), Some("everyday-word") | Some("equivalent")));
    assert!(shows_duo, "ja 耳 should surface 耳朵 (as everyday-word or curated equivalent)");
}
#[tokio::test]
async fn everyday_word_duo_flower() {
    // the 朵 "no equivalent" report: 朵 isn't Japanese, but Chinese writes the concept as 花朵
    let hit = entry_of(&search("朵").await, "zh", "朵");
    let e = get(&format!("/entry/{}", hit["lexeme_id"].as_i64().unwrap())).await.1;
    assert!(everyday_words(&e).contains(&"花朵".to_string()), "朵 should point to everyday 花朵");
}
#[tokio::test]
async fn everyday_word_relation_is_multichar_zh_with_reading() {
    // 朵 has no curated equivalent, so 花朵 stays a derived everyday-word: a good probe for the
    // relation's shape (multi-character zh).
    let hit = entry_of(&search("朵").await, "zh", "朵");
    let e = get(&format!("/entry/{}", hit["lexeme_id"].as_i64().unwrap())).await.1;
    let ew = e["translations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["relation"] == "everyday-word")
        .expect("an everyday-word link");
    assert_eq!(ew["variety"], "zh");
    assert!(ew["headword"].as_str().unwrap().chars().count() >= 2, "everyday word is multi-character");
    assert!(ew["reading"].is_string(), "everyday word carries a reading");
}
#[tokio::test]
async fn everyday_word_none_when_char_is_the_word() {
    // 山 / 人 ARE the everyday words; no compound should be suggested
    for ch in ["山", "人"] {
        let hits = search(ch).await;
        let top = hits["results"].as_array().unwrap()[0]["lexeme_id"].as_i64().unwrap();
        let e = get(&format!("/entry/{top}")).await.1;
        assert!(everyday_words(&e).is_empty(), "{ch} should NOT get an everyday-word, got {:?}", everyday_words(&e));
    }
}
#[tokio::test]
async fn everyday_word_absent_for_multichar_entry() {
    let hit = entry_of(&search("学校").await, "ja", "学校");
    let e = get(&format!("/entry/{}", hit["lexeme_id"].as_i64().unwrap())).await.1;
    assert!(everyday_words(&e).is_empty(), "multi-char words get no everyday-word pointer");
}

// Structure: components carry their MEANINGS (好 → 女 "woman" + 子 "child"), and radical-variant
// forms are glossed via their parent (a 亻-bearing char shows "person", not "radical number 9").
fn components(entry: &Value, ch_field: usize) -> Vec<(String, String)> {
    entry["characters"][ch_field]["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["ch"].as_str().unwrap().to_string(),
                c["gloss"].as_str().unwrap_or("").to_lowercase(),
            )
        })
        .collect()
}
#[tokio::test]
async fn char_components_have_meanings() {
    let hit = entry_of(&search("好").await, "zh", "好");
    let e = get(&format!("/entry/{}", hit["lexeme_id"].as_i64().unwrap())).await.1;
    let comps = components(&e, 0);
    let chs: Vec<&str> = comps.iter().map(|(c, _)| c.as_str()).collect();
    assert_eq!(chs, vec!["女", "子"], "好 decomposes into 女 + 子");
    assert!(comps.iter().any(|(c, g)| c == "女" && g.contains("woman")), "女 glossed as woman: {comps:?}");
    assert!(comps.iter().any(|(c, g)| c == "子" && g.contains("child")), "子 glossed as child: {comps:?}");
}
#[tokio::test]
async fn char_components_radical_variant_parent_gloss() {
    // 倭 = 人(semantic) + 委(phonetic). With phono-semantic role data we now surface the real,
    // lookupable component 人 ("person") glossed correctly — not the bound radical 亻 nor "radical no.".
    let hit = entry_of(&search("倭").await, "zh", "倭");
    let e = get(&format!("/entry/{}", hit["lexeme_id"].as_i64().unwrap())).await.1;
    let comps = components(&e, 0);
    assert!(
        comps.iter().any(|(c, g)| c == "人" && (g.contains("person") || g.contains("people") || g.contains("man")) && !g.contains("radical")),
        "倭 → 人 person, not 'radical number 9': {comps:?}"
    );
}
#[tokio::test]
async fn char_components_water_radical_form() {
    // 江 = 水(semantic) + 工(phonetic); the meaning component is water
    let hit = entry_of(&search("江").await, "zh", "江");
    let e = get(&format!("/entry/{}", hit["lexeme_id"].as_i64().unwrap())).await.1;
    let comps = components(&e, 0);
    assert!(comps.iter().any(|(c, g)| c == "水" && g.contains("water")), "江 → 水 water: {comps:?}");
}
#[tokio::test]
async fn char_components_repeated_base_meaning() {
    // 森 → distinct component 木 carries "tree" (the decomp says ×3; the component gives the meaning)
    let hit = entry_of(&search("森").await, "zh", "森");
    let e = get(&format!("/entry/{}", hit["lexeme_id"].as_i64().unwrap())).await.1;
    let comps = components(&e, 0);
    assert!(comps.iter().any(|(c, g)| (c == "木" || c == "林") && g.contains("tree")), "森 part means tree: {comps:?}");
}
#[tokio::test]
async fn char_components_empty_for_atomic() {
    // 木 is atomic — no sub-components to explain
    let hit = entry_of(&search("木").await, "zh", "木");
    let e = get(&format!("/entry/{}", hit["lexeme_id"].as_i64().unwrap())).await.1;
    assert!(e["characters"][0]["components"].as_array().unwrap().is_empty(), "木 is atomic");
}

// CC-CEDICT '/'-delimited senses become separate numbered senses (like JMdict), instead of
// collapsing into one "1." — so Chinese and Japanese entries enumerate uniformly.
#[tokio::test]
async fn cedict_senses_enumerate_uniformly() {
    let hit = entry_of(&search("輪").await, "zh", "輪");
    let e = get(&format!("/entry/{}", hit["lexeme_id"].as_i64().unwrap())).await.1;
    let senses = e["senses"].as_array().unwrap();
    assert!(senses.len() >= 3, "輪 zh should enumerate multiple senses, got {}", senses.len());
    // synonyms WITHIN a single CC-CEDICT sense (';') must stay joined, never over-split
    assert!(
        senses.iter().any(|s| s["gloss_en"].as_str().unwrap().contains("; ")),
        "intra-sense synonyms stay together"
    );
    // sense_order is 0,1,2… and the first sense is not the whole blob
    assert!(senses[0]["gloss_en"].as_str().unwrap().len() < 60, "first sense is one sense, not the join");
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

// C4. a ja kana reading carries its Kanjium pitch accent (箸 はし → atamadaka, downstep "1").
#[tokio::test]
async fn ja_kana_reading_has_pitch_accent() {
    let hit = entry_of(&search("箸").await, "ja", "箸");
    let id = hit["lexeme_id"].as_i64().unwrap();
    let e = get(&format!("/entry/{id}")).await.1;
    let accent = e["readings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "kana" && r["value"] == "はし")
        .and_then(|r| r["accent"].as_str());
    assert_eq!(accent, Some("1"), "箸/はし should be atamadaka (downstep 1)");
}

// C5. a SINGLE kanji's kana on/kun reading also carries the word-level Kanjium accent, so a single-
// character entry (箸/橋/端 — the minimal pairs) shows the pitch contour, not just multi-kanji words.
#[tokio::test]
async fn single_kanji_char_reading_has_pitch_accent() {
    let e = entry_top("箸").await;
    let ch = e["characters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["ch"] == "箸")
        .expect("箸 in the character breakdown");
    let accent = ch["readings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["value"] == "はし")
        .and_then(|r| r["accent"].as_str());
    assert_eq!(accent, Some("1"), "箸 kun reading はし should carry atamadaka (1) from the word lexeme");
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
// Probe batteries - the 115-word and 100-edge-case sets, distilled into invariants
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

// B1. Nothing in the batteries comes back empty - incl. kokuji with no word-lexeme (込/凪/榊,
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

// B3b. Characters that share a meaning in a NON-primary sense must not have their CROSS-LANGUAGE
// same-glyph pair flagged false-friend just because the zh and ja dictionaries lead with different
// senses (天 zh "day" / ja "sky", both mean sky+heaven; 本 zh "root" / ja "book", both mean both).
// The shared-concept override in classify_relation fixes these. (Same-LANGUAGE same-glyph pairs can
// still be false friends; the front-end warning only fires for a 2-language pair, so the
// cross-language relation is what matters.)
#[tokio::test]
async fn probe_cognate_shared_concept() {
    for (q, ja) in [("天", "天"), ("本", "本")] {
        let e = entry_of(&search(q).await, "zh", q);
        let id = e["lexeme_id"].as_i64().unwrap();
        let entry = get(&format!("/entry/{id}")).await.1;
        let sib = entry["same_form"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["variety"] == "ja" && l["headword"] == ja)
            .unwrap_or_else(|| panic!("{q} should have a ja same-glyph sibling"));
        assert_eq!(sib["relation"], "cognate", "{q} cross-language pair must be a cognate, not a false friend");
    }
}

// B4. Cross-language false friend whose forms are variant glyphs: 会社 (jp "company") and 會社
// (zh "guild") - 会 is the shinjitai of 會, so the variant-spelling cognate rule must NOT apply
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

// B7. Japanese on/kun char readings are kana - never romaji junk like "K0"/"ABURA".
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

// B9b. Compounds top/bottom split is by CONTENT, not result-cap race (item 160): on the 馬 page,
// 種馬 (which literally contains 馬) must be a top "compound", never banished to "compound-alt".
#[tokio::test]
async fn probe_compound_split_by_content() {
    let e = entry_top("馬").await;
    let comps = e["compounds"].as_array().unwrap();
    assert!(!comps.is_empty(), "馬 should list compounds");
    // 種馬 present and classified as a same-glyph compound
    let zhongma = comps
        .iter()
        .find(|l| l["headword"].as_str().unwrap() == "種馬")
        .expect("種馬 should appear on the 馬 page");
    assert_eq!(zhongma["relation"], "compound", "種馬 uses 馬, so it belongs in the top group");
    // every top row's displayed form contains the exact glyph; every alt row does NOT
    for l in comps {
        let hw = l["headword"].as_str().unwrap();
        if l["relation"] == "compound" {
            assert!(hw.contains('馬'), "top compound {hw} should contain 馬");
        } else if l["relation"] == "compound-alt" {
            assert!(!hw.contains('馬'), "variant row {hw} should not contain the exact 馬");
        }
    }
}

// B9c. All same-glyph (top) compounds precede every variant (bottom) compound, so the frontend's
// single "written with a variant" divider is correct.
#[tokio::test]
async fn probe_compound_top_before_alt() {
    let e = entry_top("馬").await;
    let comps = e["compounds"].as_array().unwrap();
    let mut seen_alt = false;
    for l in comps {
        let is_alt = l["relation"] == "compound-alt";
        if is_alt {
            seen_alt = true;
        } else {
            assert!(!seen_alt, "a same-glyph compound appeared after a variant one: ordering broken");
        }
    }
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

// B11. Cantonese 粵字 surface as 粵 with jyutping, not a nominal Mandarin reading.
#[tokio::test]
async fn probe_cantonese_yuezi() {
    let v = search("冇").await;
    let vs = varieties(&v);
    assert!(vs.contains(&"yue".to_string()), "冇 should be Cantonese, got {vs:?}");
    assert!(!vs.contains(&"zh".to_string()), "冇 should not be Mandarin");
    assert_eq!(v["results"][0]["reading"], "mou5", "冇 should read jyutping mou5");
}

// B12. Genuinely-mixed homograph keeps BOTH 中 and 粵 (乜 = surname Niè + Cantonese mat1).
#[tokio::test]
async fn probe_cantonese_mixed() {
    let vs = varieties(&search("乜").await);
    assert!(vs.contains(&"zh".to_string()) && vs.contains(&"yue".to_string()), "乜 should be both: {vs:?}");
}

// B13. A Mandarin word whose gloss only mentions Cantonese (etymology) stays 中, not relabeled.
#[tokio::test]
async fn probe_cantonese_not_overtagged() {
    let vs = varieties(&search("點心").await);
    assert!(vs.contains(&"zh".to_string()), "點心 should stay Mandarin: {vs:?}");
}

// B14. script_forms exposes the orthodox family with reform-labelled branches.
#[tokio::test]
async fn probe_script_forms_branches() {
    // 广 (simplified) → anchor 廣, branches include 广 (simplified) and 広 (shinjitai) with labels.
    let v = search("广").await;
    let id = v["results"][0]["lexeme_id"].as_i64().unwrap();
    let e = get(&format!("/entry/{id}")).await.1;
    let sf = &e["characters"][0]["script_forms"];
    assert_eq!(sf["orthodox"], "廣");
    let forms: Vec<String> = sf["branches"].as_array().unwrap().iter()
        .map(|b| b["form"].as_str().unwrap().to_string()).collect();
    assert!(forms.contains(&"廣".into()) && forms.contains(&"广".into()) && forms.contains(&"広".into()), "{forms:?}");
    let simp = sf["branches"].as_array().unwrap().iter().find(|b| b["form"] == "广").unwrap();
    assert!(simp["reform_label"].as_str().unwrap().contains("PRC"));
}

// B15. A kokuji is flagged with no Chinese branches.
#[tokio::test]
async fn probe_script_forms_kokuji() {
    let v = search("峠").await;
    let id = v["results"][0]["lexeme_id"].as_i64().unwrap();
    let e = get(&format!("/entry/{id}")).await.1;
    let sf = &e["characters"][0]["script_forms"];
    if !sf.is_null() {
        assert_eq!(sf["is_kokuji"], true, "峠 should be kokuji");
    }
}

// ── #90: homograph ranking is DETERMINISTIC (was random per request via HashMap order) ──
// helper: the first result's reading + glosses joined
fn first_reading(v: &Value) -> String {
    v["results"][0]["reading"].as_str().unwrap_or("").to_string()
}
fn first_id(v: &Value) -> i64 {
    v["results"][0]["lexeme_id"].as_i64().unwrap_or(0)
}

#[tokio::test]
async fn xi_homograph_leads_with_wash_not_xiang_ma() {
    // 洗 has xǐ ("to wash", rich) and xiǎn ("used in 洗馬", a bare cross-reference). The wash reading
    // must lead — the richer-meaning tiebreak picks it over the minor-gloss homograph.
    let v = search("洗").await;
    let g = v["results"][0]["glosses"].as_array().unwrap();
    let joined = g.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join("; ");
    assert!(
        joined.contains("wash") || joined.contains("bathe"),
        "洗 should lead with the wash reading, got: {joined}"
    );
    assert_eq!(first_reading(&v), "xi3", "lead reading should be xi3");
}

#[tokio::test]
async fn xi_ranking_is_stable_across_requests() {
    // build a fresh search (and thus a fresh HashMap) several times; the lead lexeme must not change.
    let id0 = first_id(&search("洗").await);
    for _ in 0..6 {
        assert_eq!(first_id(&search("洗").await), id0, "洗 lead lexeme must be stable across requests");
    }
}

#[tokio::test]
async fn homograph_xing_is_stable_across_requests() {
    // 行 is a multi-reading homograph (xíng / háng / héng). Whichever leads, it must be the SAME one
    // every time — no per-request flicker.
    let id0 = first_id(&search("行").await);
    for _ in 0..6 {
        assert_eq!(first_id(&search("行").await), id0, "行 lead lexeme must be deterministic");
    }
}

#[tokio::test]
async fn richer_reading_outranks_bare_cross_reference() {
    // the lead 洗 result must carry a real (non-"used in …") meaning, i.e. more than the lone xiǎn
    // cross-reference gloss.
    let v = search("洗").await;
    let g = v["results"][0]["glosses"].as_array().unwrap();
    let meaningful = g
        .iter()
        .filter_map(|x| x.as_str())
        .filter(|s| !s.trim_start().to_lowercase().starts_with("used in"))
        .count();
    assert!(meaningful >= 1, "lead reading should have a real meaning, not only a cross-reference");
}

#[tokio::test]
async fn equal_score_results_have_total_order() {
    // two 洗 lexemes share a frequency/score; the result order must be a total order (no duplicate
    // ids, deterministic) — re-running yields an identical id sequence.
    let ids = |v: &Value| -> Vec<i64> {
        v["results"].as_array().unwrap().iter().map(|r| r["lexeme_id"].as_i64().unwrap()).collect()
    };
    let a = ids(&search("洗").await);
    let b = ids(&search("洗").await);
    assert_eq!(a, b, "identical queries must return an identical result order");
}

// ── #19: per-language origin accounts (中 Sinitic + 日 Japonic for the same glyph) ──
async fn top_entry(q: &str) -> Value {
    let v = search(q).await;
    let id = v["results"][0]["lexeme_id"].as_i64().expect("a hit");
    get(&format!("/entry/{}", id)).await.1
}

#[tokio::test]
async fn shan_has_per_language_origins() {
    let e = top_entry("山").await;
    let origins = e["origins"].as_array().expect("origins array");
    assert!(origins.len() >= 2, "山 should carry >=2 language origins, got {}", origins.len());
    let vars: Vec<&str> = origins.iter().filter_map(|o| o["variety"].as_str()).collect();
    assert!(vars.contains(&"zh"), "expect a Chinese origin");
    assert!(vars.contains(&"ja"), "expect a Japanese origin");
}

#[tokio::test]
async fn origin_accounts_have_nonempty_text() {
    let e = top_entry("山").await;
    for o in e["origins"].as_array().unwrap() {
        assert!(!o["text"].as_str().unwrap_or("").is_empty(), "each origin has text");
        assert!(!o["variety"].as_str().unwrap_or("").is_empty(), "each origin has a variety");
    }
}

#[tokio::test]
async fn etymology_field_kept_for_backcompat() {
    let e = top_entry("山").await;
    assert!(e["etymology"].is_string(), "legacy etymology field still present");
}

#[tokio::test]
async fn char_entry_origins_not_thin() {
    // 山 = U+5C71 = 23665 char page now pulls origins from its word-lexemes
    let (_, e) = get("/entry/-23665").await;
    assert!(e["origins"].as_array().unwrap().len() >= 1, "char page should carry an origin");
}

#[tokio::test]
async fn first_origin_is_the_looked_up_variety() {
    let v = search("人").await;
    let top_var = v["results"][0]["variety"].as_str().unwrap().to_string();
    let id = v["results"][0]["lexeme_id"].as_i64().unwrap();
    let e = get(&format!("/entry/{}", id)).await.1;
    if let Some(first) = e["origins"].as_array().unwrap().first() {
        assert_eq!(first["variety"].as_str().unwrap(), top_var, "looked-up variety leads");
    }
}

// ── "written for sound" marker: phonetic-loan / transliteration words carry the psm badge ──
// Helper: the origin_badges of the entry for the variety whose form is exactly `q` (so a same-glyph
// hit in another language doesn't shadow the Chinese transliteration we mean to test).
async fn badges_for(q: &str) -> Vec<String> {
    let v = search(q).await;
    // pick the hit whose headword equals the query (the looked-up form), else the top hit.
    let hit = v["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["headword"] == q)
        .or_else(|| v["results"].get(0))
        .expect("a hit");
    let id = hit["lexeme_id"].as_i64().unwrap();
    let e = get(&format!("/entry/{}", id)).await.1;
    e["origin_badges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b.as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn sofa_is_phono_semantic_matching() {
    // 沙發 shāfā "sofa" — a phonetic loan; the characters were chosen for their sound.
    let b = badges_for("沙發").await;
    assert!(b.contains(&"phono-semantic-matching".to_string()), "沙發 badges = {b:?}");
}

#[tokio::test]
async fn humour_is_phono_semantic_matching() {
    // 幽默 yōumò "humour" — Hu Shih's classic psm coinage.
    let b = badges_for("幽默").await;
    assert!(b.contains(&"phono-semantic-matching".to_string()), "幽默 badges = {b:?}");
}

#[tokio::test]
async fn club_is_phono_semantic_matching() {
    // 俱樂部 "club" — a phonetic loan written with sound-and-meaning-fitting characters.
    let b = badges_for("俱樂部").await;
    assert!(b.contains(&"phono-semantic-matching".to_string()), "俱樂部 badges = {b:?}");
}

#[tokio::test]
async fn telephone_is_not_a_sound_loan() {
    // 電話 "telephone" — a normal semantic compound (electric + speech), NOT a transliteration.
    let b = badges_for("電話").await;
    assert!(
        !b.contains(&"phono-semantic-matching".to_string()),
        "電話 must not carry the sound-loan badge, got {b:?}"
    );
}

#[tokio::test]
async fn ordinary_word_has_no_sound_loan_badge() {
    // 機場 "airport" — a plain compound; no phonetic-loan signal.
    let b = badges_for("機場").await;
    assert!(
        !b.contains(&"phono-semantic-matching".to_string()),
        "機場 must not carry the sound-loan badge, got {b:?}"
    );
}

// ── #16: radical marking + appears-in characters + standalone parent ──
#[tokio::test]
async fn chuo_radical_is_flagged() {
    // 辵 = U+8FB5 = 36789
    let (_, e) = get("/entry/-36789").await;
    let c = &e["characters"][0];
    assert_eq!(c["is_radical"], true, "辵 should be flagged as a radical");
    assert_eq!(c["radical_number"], 162, "辵 is Kangxi radical 162");
}

#[tokio::test]
async fn chi_radical_appears_in_characters() {
    // 彳 = U+5F73 = 24435
    let (_, e) = get("/entry/-24435").await;
    assert_eq!(e["characters"][0]["is_radical"], true, "彳 is a radical");
    assert!(!e["appears_in"].as_array().unwrap().is_empty(), "彳 should appear in characters");
}

#[tokio::test]
async fn water_radical_variant_has_standalone() {
    // 氵 = U+6C35 = 27701 → standalone 水
    let (_, e) = get("/entry/-27701").await;
    let c = &e["characters"][0];
    assert_eq!(c["is_radical"], true);
    assert_eq!(c["standalone"], "水", "氵 stands for 水");
    let ai: Vec<&str> = e["appears_in"].as_array().unwrap().iter().filter_map(|x| x["ch"].as_str()).collect();
    assert!(ai.len() > 3, "氵 appears in many characters, got {}", ai.len());
}

#[tokio::test]
async fn common_char_with_radical_gloss_is_not_a_radical() {
    // 山 = U+5C71 = 23665: has a "Kangxi radical 46" gloss but heads thousands of words → NOT a radical
    let (_, e) = get("/entry/-23665").await;
    assert_eq!(e["characters"][0]["is_radical"], false, "山 is a real char, not a radical entry");
}

#[tokio::test]
async fn ordinary_entry_has_no_appears_in() {
    let e = top_entry("山").await;
    assert!(e["appears_in"].as_array().unwrap().is_empty(), "non-radical entry has empty appears_in");
}

// ── #17: per-character "used" count ──
#[tokio::test]
async fn used_count_separates_common_from_rare() {
    let (_, ren) = get("/entry/-20154").await; // 人 U+4EBA
    let (_, chuo) = get("/entry/-36789").await; // 辵 U+8FB5 (rare/radical)
    let ren_n = ren["characters"][0]["used_count"].as_i64().unwrap();
    let chuo_n = chuo["characters"][0]["used_count"].as_i64().unwrap();
    assert!(ren_n > 100, "人 should appear in many words, got {ren_n}");
    assert!(chuo_n <= 3, "辵 should appear in ~no words, got {chuo_n}");
    assert!(ren_n > chuo_n, "common char outranks rare char in usage");
}

#[tokio::test]
async fn used_count_present_and_nonnegative() {
    let e = top_entry("山").await;
    let n = e["characters"][0]["used_count"].as_i64().expect("used_count present");
    assert!(n >= 0, "used_count is non-negative");
    assert!(n > 50, "山 is common");
}

// ── #103: phono-semantic component roles (媽 = 女 semantic + 馬 phonetic) ──
fn comp_role<'a>(e: &'a Value, ch: &str) -> Option<&'a str> {
    e["characters"][0]["components"]
        .as_array()?
        .iter()
        .find(|c| c["ch"] == ch)
        .and_then(|c| c["role"].as_str())
}

#[tokio::test]
async fn ma_has_semantic_and_phonetic_components() {
    let e = top_entry("媽").await;
    assert_eq!(comp_role(&e, "女"), Some("semantic"), "女 carries the meaning");
    assert_eq!(comp_role(&e, "馬"), Some("phonetic"), "馬 carries the sound");
}

#[tokio::test]
async fn jiang_water_is_semantic() {
    let e = top_entry("江").await;
    assert_eq!(comp_role(&e, "水"), Some("semantic"));
    assert_eq!(comp_role(&e, "工"), Some("phonetic"));
}

#[tokio::test]
async fn ai_heart_is_semantic() {
    let e = top_entry("愛").await;
    assert_eq!(comp_role(&e, "心"), Some("semantic"));
    assert_eq!(comp_role(&e, "旡"), Some("phonetic"));
}

#[tokio::test]
async fn non_phonosemantic_char_falls_back_with_null_roles() {
    // 好 (ideogrammic 女+子) has no Han-compound role data → IDS-leaf fallback, components present,
    // every role null. (U+597D = 22909)
    let (_, e) = get("/entry/-22909").await;
    let comps = e["characters"][0]["components"].as_array().unwrap();
    assert!(!comps.is_empty(), "好 still lists components via IDS fallback");
    assert!(comps.iter().all(|c| c["role"].is_null()), "fallback components have no role");
}

#[tokio::test]
async fn component_roles_are_valid_values() {
    for q in ["媽", "江", "清", "錢", "愛"] {
        let e = top_entry(q).await;
        for c in e["characters"][0]["components"].as_array().unwrap() {
            match c["role"].as_str() {
                None | Some("semantic") | Some("phonetic") | Some("form") | Some("iconic") => {}
                other => panic!("{q}: unexpected role {other:?}"),
            }
        }
    }
}

#[tokio::test]
async fn phonetic_component_carries_its_sound() {
    let e = top_entry("媽").await;
    let comps = e["characters"][0]["components"].as_array().unwrap();
    let ma = comps.iter().find(|c| c["ch"] == "馬").unwrap();
    assert_eq!(ma["role"], "phonetic");
    // 馬's reading (tone-marked pinyin mǎ) is the sound it lends
    assert_eq!(ma["sound"].as_str(), Some("mǎ"), "馬 lends the mǎ sound, got {:?}", ma["sound"]);
    let nu = comps.iter().find(|c| c["ch"] == "女").unwrap();
    assert!(nu["sound"].is_null(), "semantic component carries no sound");
}

// ---- items 17/18: glossless components surface "appears in", rare ext-plane glyphs flagged ----
#[tokio::test]
async fn glossless_component_lists_appears_in() {
    // 𦘒 (U+26612): no gloss, no word-lexeme, but a component of 書 / 晝 / 畫.
    let (_, e) = get("/entry/-157202").await;
    assert!(e["senses"].as_array().unwrap().is_empty(), "𦘒 has no senses");
    let ai = e["appears_in"].as_array().expect("appears_in array");
    assert!(!ai.is_empty(), "𦘒 should list the characters it appears in");
    let chars: Vec<&str> = ai.iter().filter_map(|c| c["ch"].as_str()).collect();
    assert!(chars.contains(&"書"), "𦘒 appears in 書, got {chars:?}");
}

#[tokio::test]
async fn appears_in_flags_rare_extension_glyphs() {
    let (_, e) = get("/entry/-157202").await;
    let ai = e["appears_in"].as_array().unwrap();
    assert_eq!(ai[0]["rare"], false, "first appears-in glyph is common-plane, not tofu");
    assert!(ai.iter().any(|c| c["rare"] == true), "some 𦘒 hosts are rare ext-B glyphs");
}

// ---- item 13: a radical's "appears in" kanji group traditional before simplified ----
#[tokio::test]
async fn appears_in_groups_traditional_before_simplified() {
    // 钅 (U+9485, simplified metal radical): orthodox hosts must precede simplified ones.
    let (_, e) = get("/entry/-38021").await;
    let ai = e["appears_in"].as_array().expect("appears_in");
    let chars: Vec<&str> = ai.iter().filter_map(|c| c["ch"].as_str()).collect();
    match (chars.iter().position(|&c| c == "䥻"), chars.iter().position(|&c| c == "针")) {
        (Some(t), Some(s)) => assert!(t < s, "traditional 䥻 (#{t}) before simplified 针 (#{s})"),
        _ => panic!("expected both 䥻 and 针 in 钅 appears_in, got {chars:?}"),
    }
}

// ---- item 15: simplification-merge note + script on origin accounts ----
async fn zh_entry(q: &str) -> Value {
    let v = search(q).await;
    let id = v["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["variety"] == "zh")
        .expect("a zh hit")["lexeme_id"]
        .as_i64()
        .unwrap();
    get(&format!("/entry/{id}")).await.1
}

#[tokio::test]
async fn merge_glyph_origin_has_note_and_simplified_script() {
    let e = zh_entry("丑").await;
    let acc = e["origins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["variety"] == "zh")
        .expect("a zh origin for 丑");
    assert_eq!(acc["script"], "simplified", "丑 doubles as simplified 醜");
    let note = acc["note"].as_str().expect("a merge note");
    assert!(note.contains("醜"), "note names merged-in 醜, got {note:?}");
}

#[tokio::test]
async fn plain_glyph_origin_has_no_script_or_note() {
    // 山 is identical across scripts and merges nothing.
    let e = zh_entry("山").await;
    let acc = e["origins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["variety"] == "zh")
        .expect("a zh origin for 山");
    assert!(acc["script"].is_null(), "山 carries no script label");
    assert!(acc["note"].is_null(), "山 carries no merge note");
}

#[tokio::test]
async fn traditional_glyph_origin_tagged_traditional() {
    // 這 is orthodox with a simplified child 这: tagged traditional, but merges nothing.
    let e = zh_entry("這").await;
    let acc = e["origins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["variety"] == "zh")
        .expect("a zh origin for 這");
    assert_eq!(acc["script"], "traditional");
    assert!(acc["note"].is_null(), "這 merges nothing");
}

// ---- item 1: /suggest autocomplete ----
fn suggestions(v: &Value) -> Vec<String> {
    v["suggestions"].as_array().unwrap().iter().map(|s| s["headword"].as_str().unwrap().to_string()).collect()
}

#[tokio::test]
async fn suggest_han_prefix() {
    let (st, v) = get(&format!("/suggest?q={}", enc("學"))).await;
    assert_eq!(st, StatusCode::OK);
    let hw = suggestions(&v);
    assert!(!hw.is_empty(), "學 prefix yields suggestions");
    assert!(hw.iter().any(|h| h.contains('學') || h.contains('学')), "got {hw:?}");
}

#[tokio::test]
async fn suggest_pinyin_prefix() {
    let v = get(&format!("/suggest?q={}", enc("xue"))).await.1;
    let arr = v["suggestions"].as_array().unwrap();
    assert!(!arr.is_empty(), "pinyin prefix yields suggestions");
    // every suggestion carries a reading and a variety
    assert!(arr.iter().all(|s| s["variety"].is_string()));
}

#[tokio::test]
async fn suggest_english_prefix() {
    let v = get(&format!("/suggest?q={}", enc("airp"))).await.1;
    let hw = suggestions(&v);
    assert!(
        hw.iter().any(|h| ["機場", "飛機", "空港", "飛機場"].contains(&h.as_str())),
        "airp should surface an airport word, got {hw:?}"
    );
}

#[tokio::test]
async fn suggest_kana_prefix() {
    let v = get(&format!("/suggest?q={}", enc("は"))).await.1;
    let arr = v["suggestions"].as_array().unwrap();
    assert!(!arr.is_empty(), "kana prefix yields suggestions");
    assert!(arr.iter().all(|s| s["variety"] == "ja"), "は prefix is all Japanese");
}

#[tokio::test]
async fn suggest_respects_limit_and_dedupes() {
    let v = get(&format!("/suggest?q={}&limit=3", enc("學"))).await.1;
    let hw = suggestions(&v);
    assert!(hw.len() <= 3, "limit honoured, got {}", hw.len());
    let uniq: std::collections::HashSet<_> = hw.iter().collect();
    assert_eq!(uniq.len(), hw.len(), "headwords are deduped");
}

#[tokio::test]
async fn suggest_empty_query_is_empty() {
    let v = get("/suggest?q=").await.1;
    assert_eq!(v["suggestions"].as_array().unwrap().len(), 0);
}

// ---- per-language character rarity (used_by_variety) ----
#[tokio::test]
async fn char_usage_is_per_language() {
    // 巴 is common in Chinese but rare in Japanese
    let (_, e) = get(&format!("/entry/{}", -('巴' as i64))).await;
    let u = &e["characters"][0]["used_by_variety"];
    let zh = u["zh"].as_i64().unwrap_or(0);
    let ja = u["ja"].as_i64().unwrap_or(0);
    assert!(zh > ja, "巴: zh ({zh}) should exceed ja ({ja})");
    assert!(zh > 50, "巴 is common in Chinese, got {zh}");
}

#[tokio::test]
async fn kokuji_usage_is_japanese_only() {
    // 働 is a Japanese-coined kanji: used in ja, not in zh
    let (_, e) = get(&format!("/entry/{}", -('働' as i64))).await;
    let u = &e["characters"][0]["used_by_variety"];
    assert!(u["ja"].as_i64().unwrap_or(0) > 0, "働 used in Japanese");
    assert_eq!(u["zh"].as_i64().unwrap_or(0), 0, "働 not used in Chinese");
}

// ---- partial / substring word lookup (item 127) ----
#[tokio::test]
async fn partial_lookup_finds_contained_word() {
    // 犬ホテル is not a word, but contains ホテル
    let v = search("犬ホテル").await;
    let hw = headwords(&v);
    assert!(hw.contains(&"ホテル".to_string()), "expected ホテル in {hw:?}");
    let mt = v["results"][0]["match_type"].as_str().unwrap();
    assert_eq!(mt, "partial");
}

#[tokio::test]
async fn partial_lookup_ranks_longer_substrings_first() {
    let v = search("東京ホテル").await;
    let hw = headwords(&v);
    let i_hotel = hw.iter().position(|h| h == "ホテル");
    let i_tokyo = hw.iter().position(|h| h == "東京");
    if let (Some(a), Some(b)) = (i_hotel, i_tokyo) {
        assert!(a < b, "longer ホテル (#{a}) should rank before 東京 (#{b})");
    } else {
        panic!("expected both ホテル and 東京 in {hw:?}");
    }
}

#[tokio::test]
async fn whole_word_query_has_no_partial_pollution() {
    // a query that resolves to a real word must NOT also list its sub-words as partial hits
    let v = search("機場").await;
    let any_partial = v["results"].as_array().unwrap().iter().any(|r| r["match_type"] == "partial");
    assert!(!any_partial, "機場 resolves wholly; no partial matches expected");
}

#[tokio::test]
async fn single_char_query_has_no_partial() {
    let v = search("山").await;
    let any_partial = v["results"].as_array().unwrap().iter().any(|r| r["match_type"] == "partial");
    assert!(!any_partial);
}

// ---- wildcard search (item 136) ----
#[tokio::test]
async fn wildcard_prefix() {
    let v = search("你*").await;
    assert_eq!(v["classified_as"], "wildcard");
    let hw = headwords(&v);
    // matches a 你-prefixed surface form (the displayed headword may be a variant like 妳)
    assert!(hw.iter().any(|h| h == "你們"), "expected 你們 in {hw:?}");
    assert!(hw.len() >= 3);
}

#[tokio::test]
async fn wildcard_suffix() {
    let v = search("*場").await;
    let hw = headwords(&v);
    assert!(hw.iter().any(|h| h == "機場"), "*場 finds 機場: {hw:?}");
    assert!(hw.iter().all(|h| h.ends_with('場')));
}

#[tokio::test]
async fn wildcard_single_char_question() {
    let v = search("日?").await;
    let hw = headwords(&v);
    // a 2-char word starting 日 (日本); the bare single char 日 must NOT match (? needs one more char)
    assert!(hw.iter().any(|h| h == "日本"), "expected 日本 in {hw:?}");
    assert!(!hw.iter().any(|h| h == "日"), "single 日 excluded by ?");
}

#[tokio::test]
async fn wildcard_bare_star_is_empty() {
    let v = search("*").await;
    assert_eq!(v["results"].as_array().unwrap().len(), 0, "bare * must not dump the corpus");
}

#[tokio::test]
async fn wildcard_no_match_is_clean_empty() {
    let v = search("機?場").await; // no 3-char 機X場 word exists
    assert_eq!(v["classified_as"], "wildcard");
    assert_eq!(v["results"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn suggest_skips_wildcards() {
    let v = get(&format!("/suggest?q={}", enc("你*"))).await.1;
    assert_eq!(v["suggestions"].as_array().unwrap().len(), 0);
}

// ── Middle Chinese (phonological why) ──────────────────────────────────────────────────────────
// MC (廣韻 / Baxter) readings ride on char_reading kind='mc'; the char's own reading flows through
// the `readings` list, and a phonetic component's reading(s) ride on `components[].mc_sound`.
fn mc_readings(char_obj: &Value) -> Vec<String> {
    char_obj["readings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "mc")
        .map(|r| r["value"].as_str().unwrap().to_string())
        .collect()
}
async fn char0_of(headword: &str, variety: &str) -> Value {
    let hit = entry_of(&search(headword).await, variety, headword);
    let e = get(&format!("/entry/{}", hit["lexeme_id"].as_i64().unwrap())).await.1;
    e["characters"][0].clone()
}

#[tokio::test]
async fn mc_char_own_reading() {
    // 馬 = maeX in Baxter's Middle Chinese transcription (textbook value).
    let c = char0_of("馬", "zh").await;
    assert!(mc_readings(&c).contains(&"maeX".to_string()), "馬 should carry MC maeX: {:?}", mc_readings(&c));
}

#[tokio::test]
async fn mc_known_values_are_correct() {
    for (ch, want) in [("母", "muwX"), ("海", "xojX"), ("同", "duwng")] {
        let c = char0_of(ch, "zh").await;
        assert!(
            mc_readings(&c).contains(&want.to_string()),
            "{ch} should carry MC {want}: {:?}",
            mc_readings(&c)
        );
    }
}

#[tokio::test]
async fn mc_phonetic_component_carries_mc_sound() {
    // 銅 = 金 (semantic) + 同 (phonetic). The phonetic component 同 lent its sound; its MC reading
    // (duwng) should ride on that component, and it should MATCH 銅's own MC reading (real link).
    let c = char0_of("銅", "zh").await;
    let comp = c["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["ch"] == "同")
        .expect("銅 has a 同 component");
    let mc: Vec<String> =
        comp["mc_sound"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    assert!(mc.contains(&"duwng".to_string()), "同's mc_sound should include duwng: {mc:?}");
    assert!(mc_readings(&c).contains(&"duwng".to_string()), "銅 itself reads duwng: {:?}", mc_readings(&c));
}

#[tokio::test]
async fn mc_non_phonetic_component_has_no_mc_sound() {
    // The SEMANTIC component never carries mc_sound (it lent meaning, not sound): 銅's 金 (semantic).
    let c = char0_of("銅", "zh").await;
    let semantic = c["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["ch"] == "金")
        .expect("銅 has a 金 component");
    // mc_sound is skipped when empty, so it is either absent or an empty array.
    let empty = semantic.get("mc_sound").map_or(true, |v| v.as_array().map_or(true, |a| a.is_empty()));
    assert!(empty, "semantic 金 should not carry mc_sound: {:?}", semantic.get("mc_sound"));
}

#[tokio::test]
async fn mc_media_no_false_link_for_ma() {
    // 媽 is a LATE character: in 廣韻 it read muX (like 母), NOT like its modern phonetic 馬 (maeX).
    // We must surface both honestly and never fabricate a match. 媽's own MC ≠ 馬's MC.
    let c = char0_of("媽", "zh").await;
    let ma_self = mc_readings(&c);
    assert!(ma_self.contains(&"muX".to_string()), "媽 reads muX in 廣韻: {ma_self:?}");
    // its phonetic component 馬 carries maeX, which is genuinely different (no shared reading).
    if let Some(comp) = c["components"].as_array().unwrap().iter().find(|x| x["ch"] == "馬") {
        let mc: Vec<String> =
            comp["mc_sound"].as_array().unwrap_or(&vec![]).iter().map(|v| v.as_str().unwrap().to_string()).collect();
        assert!(mc.contains(&"maeX".to_string()), "馬's mc_sound is maeX: {mc:?}");
        assert!(!ma_self.iter().any(|r| mc.contains(r)), "媽 and 馬 must NOT share an MC reading: {ma_self:?} vs {mc:?}");
    }
}

// ── /segment: maximal-munch "literally" composition (longest known sub-words) ─────────────────────
async fn segment(q: &str) -> Value {
    get(&format!("/segment?q={}", enc(q))).await.1
}
fn seg_forms(v: &Value) -> Vec<String> {
    v["segments"].as_array().unwrap().iter().map(|s| s["form"].as_str().unwrap().to_string()).collect()
}
fn seg_glosses(v: &Value) -> Vec<String> {
    v["segments"].as_array().unwrap().iter().map(|s| s["gloss"].as_str().unwrap().to_string()).collect()
}

#[tokio::test]
async fn segment_char_plus_word() {
    // the headline case: 紅出口 must read 紅(char "red") + 出口(word "exit"), NOT 紅·出·口.
    let v = segment("紅出口").await;
    assert_eq!(seg_forms(&v), vec!["紅", "出口"], "greedy split of 紅出口");
    assert_eq!(seg_glosses(&v), vec!["red", "exit"], "composed gloss");
    // the multi-char word carries its lexeme id; the single-char fallback does not
    let segs = v["segments"].as_array().unwrap();
    assert!(segs[0].get("lexeme_id").is_none(), "紅 is a character fallback (no lexeme_id)");
    assert!(segs[1]["lexeme_id"].is_i64(), "出口 is a known word (has lexeme_id)");
}

#[tokio::test]
async fn segment_two_word_split() {
    // a clean 2+2 split: 機場(airport) + 出口(exit), the whole string is not itself a word.
    let v = segment("機場出口").await;
    assert_eq!(seg_forms(&v), vec!["機場", "出口"]);
    assert_eq!(seg_glosses(&v), vec!["airport", "exit"]);
}

#[tokio::test]
async fn segment_whole_string_is_one_word() {
    // a fully-known word returns a single segment spanning it (火車 = train).
    let v = segment("火車").await;
    assert_eq!(seg_forms(&v), vec!["火車"]);
    assert_eq!(seg_glosses(&v), vec!["train"]);
}

#[tokio::test]
async fn segment_per_char_fallback() {
    // no multi-char sub-word (紅口 is not a word): falls back to one segment per character, like today.
    let v = segment("紅口").await;
    assert_eq!(seg_forms(&v), vec!["紅", "口"]);
    assert_eq!(seg_glosses(&v), vec!["red", "mouth"]);
    for s in v["segments"].as_array().unwrap() {
        assert!(s.get("lexeme_id").is_none(), "character fallbacks carry no lexeme_id");
    }
}

#[tokio::test]
async fn segment_gloss_is_cleaned() {
    // 出口's sense-0 gloss is "an exit"; the segment gloss must be cleaned to "exit" (no article).
    let v = segment("出口").await;
    assert_eq!(seg_glosses(&v), vec!["exit"], "leading article stripped");
}

#[tokio::test]
async fn segment_non_han_is_empty() {
    // a non-Han query has nothing to compose; segments is empty (the literal line won't render).
    let v = segment("hello").await;
    assert_eq!(v["segments"].as_array().unwrap().len(), 0);
}

// ── stress-test regressions: no 500s on edge inputs ──────────────────────────────────────────────
#[tokio::test]
async fn segment_null_gloss_char_no_500() {
    // 㐃 (U+3403) is a Han char whose character-table gloss_en is NULL; segmenting must not 500.
    let (st, v) = get(&format!("/segment?q={}", enc("㐃"))).await;
    assert_eq!(st, StatusCode::OK, "segment of a NULL-gloss char must be 200, not 500");
    assert_eq!(seg_forms(&v), vec!["㐃"]);
}

#[tokio::test]
async fn suggest_fts_keyword_no_500() {
    // bare FTS5 keywords as autocomplete input must not crash (OR* was an FTS syntax error → 500).
    for q in ["OR", "NOT", "AND", "NEAR"] {
        let (st, _) = get(&format!("/suggest?q={}", enc(q))).await;
        assert_eq!(st, StatusCode::OK, "/suggest?q={q} should be 200");
    }
}

#[tokio::test]
async fn segment_parenthetical_sense_word() {
    // 下挫's sense-0 gloss is wholly parenthetical ("(of sales, prices etc) to fall"); the segmenter
    // must recover a usable gloss (strip the balanced paren, or fall to a later sense) and keep it as
    // ONE known word, not split into 下·挫.
    let v = segment("下挫").await;
    assert_eq!(seg_forms(&v), vec!["下挫"]);
}

#[tokio::test]
async fn rare_forms_hidden_from_display() {
    // 会う's rare alt-form 遇う (JMdict rK) must NOT show among its displayed forms (it stays its own
    // searchable entry, just not a normal variant of 会う).
    let v = search("会う").await;
    let hit = v["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["headword"] == "会う")
        .expect("会う present");
    let forms: Vec<String> =
        hit["forms"].as_array().unwrap().iter().map(|f| f["form"].as_str().unwrap().to_string()).collect();
    assert!(forms.contains(&"会う".to_string()), "primary form present");
    assert!(!forms.contains(&"遇う".to_string()), "rare rK form 遇う must be hidden: {forms:?}");
}
