//! OCR from image/camera (PaddleOCR PP-OCRv5 mobile via oar-ocr/ONNX).
//!
//! POST raw image bytes to `/ocr`; returns recognized lines with axis-aligned boxes, each split
//! into per-character cells (Han is ~monospace) so the frontend can overlay the image and let the
//! user tap/drag to select the character(s) to look up. Engine + models load once at startup;
//! if the models are missing the endpoint returns 503 and the rest of the app is unaffected.

use std::sync::Arc;

use axum::{body::Bytes, extract::State, http::StatusCode, Json};
use oar_ocr::prelude::{OAROCRBuilder, OAROCR};
use serde_json::{json, Value};

use crate::model::{OcrChar, OcrLine, OcrResponse};
use crate::state::AppState;

/// Build the OCR engine from the model files (dir via KOGU_OAR_DIR). Returns None if unavailable,
/// so the server still runs without OCR.
pub fn load_engine() -> Option<OAROCR> {
    let dir = std::env::var("KOGU_OAR_DIR")
        .unwrap_or_else(|_| "/mnt/HC_Volume_102319212/wenbun/oar".to_string());
    let det = format!("{dir}/pp-ocrv5_mobile_det.onnx");
    let rec = format!("{dir}/pp-ocrv5_mobile_rec.onnx");
    let dict = format!("{dir}/ppocrv5_dict.txt");
    if !std::path::Path::new(&det).exists() {
        tracing::warn!("OCR models not found in {dir} — /ocr disabled");
        return None;
    }
    let mut builder = OAROCRBuilder::new(det, rec, dict);
    // text-line orientation classification: rotates rotated/vertical lines before recognition
    let ori = format!("{dir}/pp-lcnet_x0_25_textline_ori.onnx");
    if std::path::Path::new(&ori).exists() {
        builder = builder.with_text_line_orientation_classification(ori);
        tracing::info!("OCR: text-line orientation classification enabled");
    }
    match builder.build() {
        Ok(engine) => {
            tracing::info!("OCR engine loaded from {dir}");
            Some(engine)
        }
        Err(e) => {
            tracing::warn!("OCR engine failed to load ({e}) — /ocr disabled");
            None
        }
    }
}

pub async fn ocr_handler(
    State(st): State<AppState>,
    body: Bytes,
) -> Result<Json<OcrResponse>, (StatusCode, Json<Value>)> {
    let engine = match &st.ocr {
        Some(e) => e.clone(),
        None => {
            return Err((StatusCode::SERVICE_UNAVAILABLE, Json(json!({ "error": "ocr_unavailable" }))))
        }
    };
    if body.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "empty_body" }))));
    }
    // OCR is CPU-heavy + blocking — run off the async runtime.
    tokio::task::spawn_blocking(move || run_ocr(&engine, &body))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?
        .map(Json)
}

fn run_ocr(engine: &OAROCR, bytes: &[u8]) -> Result<OcrResponse, (StatusCode, Json<Value>)> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({ "error": "bad_image", "detail": e.to_string() }))))?
        .to_rgb8();
    let (width, height) = (img.width(), img.height());

    let results = engine.predict(vec![img]).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "ocr_failed", "detail": e.to_string() })))
    })?;

    let mut lines = Vec::new();
    if let Some(result) = results.into_iter().next() {
        for region in result.text_regions {
            let text = match region.text {
                Some(t) => t.to_string(),
                None => continue,
            };
            if text.trim().is_empty() {
                continue;
            }
            let pts = &region.bounding_box.points;
            if pts.is_empty() {
                continue;
            }
            let (mut minx, mut miny, mut maxx, mut maxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for p in pts {
                minx = minx.min(p.x);
                miny = miny.min(p.y);
                maxx = maxx.max(p.x);
                maxy = maxy.max(p.y);
            }
            let box_ = [minx, miny, maxx - minx, maxy - miny];
            let chars = char_cells(box_, &text);
            lines.push(OcrLine {
                text,
                confidence: region.confidence.unwrap_or(0.0),
                box_,
                chars,
            });
        }
    }
    Ok(OcrResponse { width, height, lines })
}

/// Convenience for AppState to hold a shareable engine.
pub type SharedOcr = Option<Arc<OAROCR>>;

/// Split a line's bounding box into per-character cells. Whitespace is dropped (no geometry);
/// a tall CJK line (h > 1.5·w, >1 char) is treated as vertical text and split top-to-bottom.
/// Pure + deterministic so it can be unit-tested without the OCR engine.
pub fn char_cells(box_: [f32; 4], text: &str) -> Vec<OcrChar> {
    let [x, y, w, h] = box_;
    let chars: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    let n = chars.len();
    if n == 0 {
        return Vec::new();
    }
    let vertical = h > w * 1.5 && n > 1;
    chars
        .iter()
        .enumerate()
        .map(|(i, ch)| {
            let b = if vertical {
                let cell = h / n as f32;
                [x, y + cell * i as f32, w, cell]
            } else {
                let cell = w / n as f32;
                [x + cell * i as f32, y, cell, h]
            };
            OcrChar { ch: ch.to_string(), box_: b }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::char_cells;

    #[test]
    fn horizontal_split_even() {
        let cells = char_cells([0.0, 0.0, 100.0, 20.0], "abcd");
        assert_eq!(cells.len(), 4);
        assert_eq!(cells[0].box_, [0.0, 0.0, 25.0, 20.0]);
        assert_eq!(cells[1].box_, [25.0, 0.0, 25.0, 20.0]);
        assert_eq!(cells[3].box_, [75.0, 0.0, 25.0, 20.0]);
        assert_eq!(cells[2].ch, "c");
    }

    #[test]
    fn vertical_split_for_tall_cjk_line() {
        // tall box (h >> w) with 2 chars -> split top-to-bottom
        let cells = char_cells([10.0, 0.0, 20.0, 100.0], "上下");
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].box_, [10.0, 0.0, 20.0, 50.0]);
        assert_eq!(cells[1].box_, [10.0, 50.0, 20.0, 50.0]);
    }

    #[test]
    fn single_char_fills_box() {
        let cells = char_cells([5.0, 6.0, 30.0, 30.0], "字");
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].box_, [5.0, 6.0, 30.0, 30.0]); // not treated as vertical
    }

    #[test]
    fn whitespace_dropped() {
        let cells = char_cells([0.0, 0.0, 100.0, 20.0], "a b c");
        assert_eq!(cells.len(), 3);
        assert_eq!(cells[0].box_[2], 100.0 / 3.0); // width split by non-space count
        assert!(cells.iter().all(|c| c.ch != " "));
    }

    #[test]
    fn empty_and_whitespace_only_yield_nothing() {
        assert!(char_cells([0.0, 0.0, 10.0, 10.0], "").is_empty());
        assert!(char_cells([0.0, 0.0, 10.0, 10.0], "   ").is_empty());
    }

    #[test]
    fn wide_two_char_line_is_horizontal_not_vertical() {
        // wide box (w >> h) must split left-to-right even with 2 chars
        let cells = char_cells([0.0, 0.0, 200.0, 20.0], "中文");
        assert_eq!(cells[0].box_, [0.0, 0.0, 100.0, 20.0]);
        assert_eq!(cells[1].box_, [100.0, 0.0, 100.0, 20.0]);
    }
}
