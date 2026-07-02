//! OpenAPI document for the public API (code-first via utoipa).
//!
//! The spec is generated OFFLINE by `cargo run --release --bin dump_openapi` and shipped as a
//! static `frontend/public/api-docs/openapi.json` rendered by a self-hosted Scalar page; the
//! router itself does not serve it. Paths are described WITHOUT the `/api` prefix nginx adds on
//! the public site; the `servers` URL carries the prefix instead.

use utoipa::OpenApi;

const DESCRIPTION: &str = "\
Free public read-only API of [kogu](https://kogu.miro.build), an open-source CJKV dictionary \
covering Mandarin (zh), Cantonese (yue) and Japanese (ja).

- No authentication; requests are rate-limited by nginx. Be gentle and cache what you fetch.
- Dictionary content is licensed CC BY-SA 4.0 (aggregating CC-CEDICT, JMdict, Unihan, \
Wiktionary, Kanjium and other sources); see NOTICE.md in the repository for attribution details.
- `/recognize` and `/mt` proxy third-party services (Google handwriting input / translation): \
they exist for the app UI and are NOT stable data endpoints; do not build on them.
- Lexeme ids come from `/search` or `/suggest` and are stable within one database build, but may \
change between data releases; treat them as opaque and re-resolve via search.";

#[derive(OpenApi)]
#[openapi(
    info(title = "Kogu API", description = DESCRIPTION),
    servers((url = "https://kogu.miro.build/api", description = "Public server (nginx adds the /api prefix)")),
    paths(
        crate::handlers::health,
        crate::handlers::search_handler,
        crate::handlers::suggest_handler,
        crate::handlers::entry_handler,
        crate::handlers::why_handler,
        crate::handlers::translate_handler,
        crate::handlers::segment_handler,
        crate::mt::translate_handler,
        crate::recognize::recognize_handler,
        crate::ocr::ocr_handler,
        crate::tts::ja_handler,
        crate::tts::clip_handler,
    ),
    components(schemas(
        crate::model::SearchResponse,
        crate::model::SuggestResponse,
        crate::model::SuggestItem,
        crate::model::Hit,
        crate::model::Form,
        crate::model::Entry,
        crate::model::OriginAccount,
        crate::model::CharLite,
        crate::model::ReadingKV,
        crate::model::Sense,
        crate::model::CharInfo,
        crate::model::Component,
        crate::model::CharDecomp,
        crate::model::VariantEdge,
        crate::model::ScriptForms,
        crate::model::FormBranch,
        crate::model::WhyResponse,
        crate::model::LinkLite,
        crate::model::TranslateResponse,
        crate::model::ConceptGroup,
        crate::model::SegmentResponse,
        crate::model::SegmentPart,
        crate::model::OcrResponse,
        crate::model::OcrLine,
        crate::model::OcrChar,
        crate::model::ApiError,
        crate::recognize::RecognizeRequest,
        crate::recognize::RecognizeResponse,
    )),
    tags(
        (name = "meta", description = "Service metadata"),
        (name = "dictionary", description = "Dictionary lookups over the kogu database"),
        (name = "input", description = "Input helpers (handwriting, OCR, machine translation)"),
        (name = "audio", description = "Pronunciation audio"),
    )
)]
pub struct ApiDoc;

#[cfg(test)]
mod tests {
    use super::ApiDoc;
    use utoipa::OpenApi;

    #[test]
    fn spec_serializes() {
        ApiDoc::openapi().to_pretty_json().expect("spec must serialize");
    }
}
