//! OpenAPI document for the public API (code-first via utoipa).
//!
//! The spec is generated OFFLINE by `cargo run --release --bin dump_openapi` and shipped as a
//! static `frontend/public/api-docs/openapi.json` rendered by a self-hosted Scalar page; the
//! router itself does not serve it. Paths are described WITHOUT the `/api` prefix nginx adds on
//! the public site; the `servers` URL carries the prefix instead.
//!
//! Deliberately NOT documented (they exist for the app UI only): `/recognize` and `/mt` proxy
//! Google services, `/ocr` runs the local recognizer on uploads. None of them are dictionary
//! data, so they stay out of the public reference.

use utoipa::OpenApi;

const DESCRIPTION: &str = "\
The JSON API behind [kogu](https://kogu.miro.build), an open-source dictionary of Mandarin (zh), \
Cantonese (yue) and Japanese (ja). Read-only, no keys, rate-limited.

The dictionary content is compiled from open datasets (CC-CEDICT, JMdict, Unihan, Wiktionary, \
Kanjium and others) and is licensed CC BY-SA 4.0; NOTICE.md in the repository has the full list.

Lexeme ids are stable within one database build but can change when the data is rebuilt, so \
treat them as opaque and re-resolve through `/search` when in doubt.";

#[derive(OpenApi)]
#[openapi(
    info(title = "Kogu API", description = DESCRIPTION),
    servers((url = "https://kogu.miro.build/api", description = "nginx adds the /api prefix")),
    paths(
        crate::handlers::health,
        crate::handlers::search_handler,
        crate::handlers::suggest_handler,
        crate::handlers::interesting_handler,
        crate::handlers::entry_handler,
        crate::handlers::random_handler,
        crate::handlers::why_handler,
        crate::handlers::translate_handler,
        crate::handlers::segment_handler,
        crate::tts::ja_handler,
        crate::tts::clip_handler,
    ),
    components(schemas(
        crate::model::SearchResponse,
        crate::model::SuggestResponse,
        crate::model::SuggestItem,
        crate::model::InterestingResponse,
        crate::model::InterestingItem,
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
        crate::model::ApiError,
    )),
    tags(
        (name = "meta", description = "Service metadata"),
        (name = "dictionary", description = "Lookups over the kogu database"),
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
