//! Dump the OpenAPI spec as pretty JSON to stdout.
//!
//! Usage: `cargo run --release --bin dump_openapi > ../frontend/public/api-docs/openapi.json`

use utoipa::OpenApi;

fn main() {
    let json = kogu::openapi::ApiDoc::openapi()
        .to_pretty_json()
        .expect("serialize OpenAPI spec");
    println!("{json}");
}
