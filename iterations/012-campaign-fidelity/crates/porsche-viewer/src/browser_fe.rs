//! Front-end data for the browser menus: original `.lay` layouts, FSH shapes and FFN fonts.
//! The page draws them on a 640x480 canvas (`web/frontend.js`); see `docs/re/frontend.md`.
use js_sys::{Array, Object, Reflect, Uint8Array, Uint8ClampedArray};
use wasm_bindgen::prelude::*;

fn set(obj: &Object, key: &str, value: &JsValue) -> Result<(), JsValue> {
    Reflect::set(obj, &JsValue::from_str(key), value).map(|_| ())
}

/// A layout as JSON: `[{"index":N,"attrs":{"key":"value"}}]` in file order (keys lower-case).
#[wasm_bindgen]
pub fn fe_parse_layout(bytes: Uint8Array) -> Result<String, JsValue> {
    nfs_formats::parse_lay(&bytes.to_vec())
        .map(|lay| nfs_formats::layout_json(&lay))
        .map_err(|e| JsValue::from_str(&e))
}

/// Every shape of an FSH pack, in directory order:
/// `{index, name, width, height, x, y, rgba}`; `rgba` is null for a shape that fails to decode.
#[wasm_bindgen]
pub fn fe_decode_fsh(bytes: Uint8Array) -> Result<Array, JsValue> {
    let entries =
        nfs_formats::parse_fsh_entries(&bytes.to_vec()).map_err(|e| JsValue::from_str(&e))?;
    let out = Array::new();
    for e in entries {
        let obj = Object::new();
        set(&obj, "index", &JsValue::from(e.index as u32))?;
        set(&obj, "name", &JsValue::from_str(&e.name))?;
        set(&obj, "width", &JsValue::from(e.width))?;
        set(&obj, "height", &JsValue::from(e.height))?;
        set(&obj, "cx", &JsValue::from(e.center.0))?;
        set(&obj, "cy", &JsValue::from(e.center.1))?;
        match &e.image {
            Ok(img) => {
                set(&obj, "x", &JsValue::from(img.x))?;
                set(&obj, "y", &JsValue::from(img.y))?;
                set(
                    &obj,
                    "rgba",
                    &Uint8ClampedArray::from(img.rgba.as_slice()).into(),
                )?;
            }
            Err(err) => {
                set(&obj, "rgba", &JsValue::NULL)?;
                set(&obj, "error", &JsValue::from_str(err))?;
            }
        }
        out.push(&obj);
    }
    Ok(out)
}

/// An FFN font: `{lineHeight, width, height, alpha, glyphs: [[code, w, h, x, y, advance, xoff, yoff]]}`.
#[wasm_bindgen]
pub fn fe_parse_font(bytes: Uint8Array) -> Result<Object, JsValue> {
    let font = nfs_formats::parse_ffn(&bytes.to_vec()).map_err(|e| JsValue::from_str(&e))?;
    let obj = Object::new();
    set(&obj, "lineHeight", &JsValue::from(font.line_height))?;
    set(&obj, "width", &JsValue::from(font.atlas_width))?;
    set(&obj, "height", &JsValue::from(font.atlas_height))?;
    set(
        &obj,
        "alpha",
        &Uint8Array::from(font.alpha.as_slice()).into(),
    )?;
    let glyphs = Array::new();
    for g in &font.glyphs {
        let row = Array::new();
        for v in [
            g.code as i32,
            g.width as i32,
            g.height as i32,
            g.x as i32,
            g.y as i32,
            g.advance as i32,
            g.x_offset as i32,
            g.y_offset as i32,
        ] {
            row.push(&JsValue::from(v));
        }
        glyphs.push(&row);
    }
    set(&obj, "glyphs", &glyphs)?;
    Ok(obj)
}

/// One language of a `.loc` string table as JSON `{"id": "text", ...}` (0 = English).
#[wasm_bindgen]
pub fn fe_parse_strings(bytes: Uint8Array, language: u32) -> Result<String, JsValue> {
    let table = nfs_formats::parse_loc(&bytes.to_vec(), language as usize)
        .map_err(|e| JsValue::from_str(&e))?;
    let mut out = String::from("{");
    for (i, (id, text)) in table.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!("\"{id}\":\""));
        for c in text.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
    }
    out.push('}');
    Ok(out)
}

/// Track table of `nfs5.trk` as a JSON array.
#[wasm_bindgen]
pub fn fe_track_catalog_json(bytes: Uint8Array) -> Result<String, JsValue> {
    let t = nfs_formats::parse_track_catalog(&bytes.to_vec()).map_err(|e| JsValue::from_str(&e))?;
    Ok(nfs_formats::track_catalog_json(&t))
}

/// The Evolution event records of `nfs5.trn` as the event info screen reads them, as JSON.
#[wasm_bindgen]
pub fn fe_event_infos_json(bytes: Uint8Array) -> Result<String, JsValue> {
    let e = nfs_formats::parse_event_infos(&bytes.to_vec()).map_err(|e| JsValue::from_str(&e))?;
    Ok(nfs_formats::event_infos_json(&e))
}

/// Record indices of the original Quick Race car list from `nfs5.car`, as a JSON array.
#[wasm_bindgen]
pub fn fe_quick_race_cars_json(bytes: Uint8Array, cheat_all: Option<bool>) -> Result<String, JsValue> {
    let c = nfs_formats::parse_car_catalogue(&bytes.to_vec()).map_err(|e| JsValue::from_str(&e))?;
    Ok(if cheat_all == Some(true) {
        nfs_formats::all_cars_json(&c)
    } else {
        nfs_formats::quick_race_json(&c)
    })
}

/// The opponents of a Quick Race: `count` distinct drivers of `Game.Difficutly` `level` from
/// `nfs5.per`, as a JSON array `[{id, name_string, skill: [8 floats]}]` (`FUN_004e3160`).
/// `seed` drives the draw (the original seeds `rand()` with the time).
#[wasm_bindgen]
pub fn fe_pick_opponents_json(bytes: Uint8Array, count: u32, level: i32, seed: u32) -> String {
    use nfs_assets::race::original_ai::roster::{parse_personalities, pick_opponents};
    let drivers = parse_personalities(&bytes.to_vec());
    // MSVC `rand()`.
    let mut state = seed;
    let picked = pick_opponents(&drivers, count as usize, Some(level as i16), || {
        state = state.wrapping_mul(214013).wrapping_add(2531011);
        (state >> 16) & 0x7fff
    });
    let items: Vec<String> = picked
        .iter()
        .map(|d| {
            let skill: Vec<String> = d.skill_154.iter().map(|v| format!("{v}")).collect();
            format!(
                "{{\"id\":{},\"name_string\":{},\"skill\":[{}]}}",
                d.id,
                d.name_string,
                skill.join(",")
            )
        })
        .collect();
    format!("[{}]", items.join(","))
}
