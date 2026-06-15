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
    // tests (and any non-OCR deployment) can skip the ONNX runtime entirely — loading it without
    // ORT_DYLIB_PATH blocks, and the search/entry API doesn't need it.
    if std::env::var("KOGU_SKIP_OCR").is_ok() {
        tracing::info!("KOGU_SKIP_OCR set — /ocr disabled");
        return None;
    }
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

/// One recognized region: text, confidence, and an axis-aligned box [x,y,w,h].
struct Region {
    text: String,
    conf: f32,
    box_: [f32; 4],
}

fn recognize(engine: &OAROCR, img: image::RgbImage) -> Result<Vec<Region>, (StatusCode, Json<Value>)> {
    let results = engine.predict(vec![img]).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "ocr_failed", "detail": e.to_string() })))
    })?;
    let mut out = Vec::new();
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
            out.push(Region {
                text,
                conf: region.confidence.unwrap_or(0.0),
                box_: [minx, miny, maxx - minx, maxy - miny],
            });
        }
    }
    Ok(out)
}

/// Total recognized "evidence" = Σ chars × confidence. Used to pick the better orientation.
fn score(regions: &[Region]) -> f32 {
    regions.iter().map(|r| r.text.chars().filter(|c| !c.is_whitespace()).count() as f32 * r.conf).sum()
}

fn run_ocr(engine: &OAROCR, bytes: &[u8]) -> Result<OcrResponse, (StatusCode, Json<Value>)> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({ "error": "bad_image", "detail": e.to_string() }))))?
        .to_rgb8();
    let (width, height) = (img.width(), img.height());

    // Pass 1: upright. PP-OCR reads horizontal text well.
    let upright = recognize(engine, img.clone())?;
    let best_conf = upright.iter().map(|r| r.conf).fold(0.0_f32, f32::max);

    // If upright is weak, the text may be vertical: OCR a 90° (CCW) rotation — vertical columns
    // become horizontal lines — and keep whichever orientation has more evidence. Boxes from the
    // rotated pass are mapped back to the original frame (they become tall = vertical, so char_cells
    // splits them top-to-bottom).
    let regions = if best_conf >= 0.6 {
        upright
    } else {
        let rotated = recognize(engine, image::imageops::rotate270(&img))?;
        if score(&rotated) > score(&upright) {
            rotated.into_iter().map(|r| map_from_rot270(r, height)).collect()
        } else {
            upright
        }
    };

    let lines = regions
        .into_iter()
        .map(|r| OcrLine { chars: char_cells(r.box_, &r.text), text: r.text, confidence: r.conf, box_: r.box_ })
        .collect();
    Ok(OcrResponse { width, height, lines })
}

/// Map a region from a rotate270 (CCW) image back to original coordinates. rotate270 maps original
/// (x,y) -> rotated (y, W-1-x); inverse of an aabb (bx,by,bw,bh) with original height H is
/// [H-(by+bh), bx, bh, bw]. Text read left-to-right in the rotated frame is top-to-bottom in the
/// original column, so order is preserved.
fn map_from_rot270(r: Region, orig_height: u32) -> Region {
    let [bx, by, bw, bh] = r.box_;
    Region {
        box_: [orig_height as f32 - (by + bh), bx, bh, bw],
        ..r
    }
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
    use super::{char_cells, map_from_rot270, Region};

    #[test]
    fn rot270_box_maps_to_tall_vertical_box() {
        // a horizontal line in the rotated frame becomes a tall (vertical) box in the original
        let r = Region { text: "日本語".into(), conf: 1.0, box_: [0.0, 0.0, 90.0, 30.0] };
        let m = map_from_rot270(r, 200);
        // [H-(by+bh), bx, bh, bw] = [200-30, 0, 30, 90]
        assert_eq!(m.box_, [170.0, 0.0, 30.0, 90.0]);
        assert!(m.box_[3] > m.box_[2], "mapped box should be tall (vertical)");
        assert_eq!(m.text, "日本語"); // text/conf preserved
    }

    #[test]
    fn rot270_offset_box() {
        let r = Region { text: "x".into(), conf: 0.9, box_: [10.0, 20.0, 5.0, 40.0] };
        let m = map_from_rot270(r, 100);
        assert_eq!(m.box_, [40.0, 10.0, 40.0, 5.0]); // [100-(20+40), 10, 40, 5]
    }


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
