//! Native PNG/PDF export, optional so font/raster dependencies stay out of the core Wasm bundle.
use crate::Error;

use std::collections::BTreeMap;

fn parse_root_vars(svg: &str) -> BTreeMap<String, String> {
    let mut vars = BTreeMap::new();
    let Some(svg_start) = svg.find("<svg") else {
        return vars;
    };
    let Some(svg_end) = svg[svg_start..].find('>').map(|n| svg_start + n) else {
        return vars;
    };
    let tag = &svg[svg_start..svg_end];
    let Some(style_start) = tag.find(" style=\"").map(|n| n + 8) else {
        return vars;
    };
    let Some(style_end) = tag[style_start..].find('"').map(|n| style_start + n) else {
        return vars;
    };
    for decl in tag[style_start..style_end].split(';') {
        if let Some((k, v)) = decl.split_once(':') {
            let key = k.trim();
            let val = v.trim();
            if key.starts_with("--sd-") && !val.is_empty() {
                vars.insert(key.to_owned(), val.to_owned());
            }
        }
    }
    vars
}

fn color_fallbacks(value: &str, overrides: &BTreeMap<String, String>) -> String {
    let mut value = value.to_owned();
    let mut pos = 0;
    while let Some(start) = value[pos..].find("var(--sd-").map(|n| pos + n) {
        let Some(end) = value[start..].find(')').map(|n| start + n) else {
            break;
        };
        let inner = &value[start + 4..end];
        let replacement = if let Some((name, fallback)) = inner.split_once(',') {
            overrides
                .get(name.trim())
                .cloned()
                .unwrap_or_else(|| fallback.trim().to_owned())
        } else if let Some(val) = overrides.get(inner.trim()) {
            val.clone()
        } else {
            pos = end + 1;
            continue;
        };
        value.replace_range(start..=end, &replacement);
        pos = start + replacement.len();
    }
    value
}

// Resolve only paint attributes and style declarations. A diagram caption can
// itself contain CSS syntax, which must remain literal text in native exports.
fn native_svg(svg: &str) -> String {
    let overrides = parse_root_vars(svg);
    let mut output = String::with_capacity(svg.len());
    let mut cursor = 0;
    while let Some(open) = svg[cursor..].find('<').map(|n| cursor + n) {
        output.push_str(&svg[cursor..open]);
        let Some(end) = svg[open..].find('>').map(|n| open + n + 1) else {
            cursor = open;
            break;
        };
        let mut tag = svg[open..end].to_owned();
        for attribute in [" fill=\"", " stroke=\"", " style=\""] {
            let mut offset = 0;
            while let Some(start) = tag[offset..]
                .find(attribute)
                .map(|n| offset + n + attribute.len())
            {
                let Some(end) = tag[start..].find('"').map(|n| start + n) else {
                    break;
                };
                let fallback = color_fallbacks(&tag[start..end], &overrides);
                tag.replace_range(start..end, &fallback);
                offset = start + fallback.len() + 1;
            }
        }
        output.push_str(&tag);
        cursor = end;
        if tag.starts_with("<style")
            && let Some(end) = svg[cursor..].find("</style>").map(|n| cursor + n)
        {
            output.push_str(&color_fallbacks(&svg[cursor..end], &overrides));
            cursor = end;
        }
    }
    output.push_str(&svg[cursor..]);
    output
}

fn tree(svg: &str) -> Result<resvg::usvg::Tree, Error> {
    use resvg::usvg::{Options, fontdb};
    let mut options = Options::default();
    let fonts = options.fontdb_mut();
    fonts.load_system_fonts();
    // fontdb's default generic family can name an absent font (for example
    // Arial on Linux). Loading faces alone then silently drops every glyph.
    let family = [
        "Inter",
        "DejaVu Sans",
        "Liberation Sans",
        "Noto Sans",
        "Arial",
        "Helvetica",
    ]
    .into_iter()
    .find(|name| {
        fonts
            .query(&fontdb::Query {
                families: &[fontdb::Family::Name(name)],
                ..Default::default()
            })
            .is_some()
    })
    .map(str::to_owned)
    .or_else(|| {
        fonts
            .faces()
            .find_map(|face| face.families.first().map(|f| f.0.clone()))
    })
    .ok_or_else(|| {
        Error::Layout("native text export requires an installed TrueType/OpenType font".into())
    })?;
    fonts.set_sans_serif_family(family.clone());
    options.font_family = family;
    // Native exporters do not implement browser custom properties. Resolve our
    // explicit light fallbacks while preserving arbitrary user colors.
    let native = native_svg(svg);
    resvg::usvg::Tree::from_str(&native, &options).map_err(|e| Error::Layout(e.to_string()))
}

pub fn png(svg: &str, scale: f32) -> Result<Vec<u8>, Error> {
    if !scale.is_finite() || scale <= 0. || scale > 8. {
        return Err(Error::Edit("PNG scale must be in (0,8]".into()));
    }
    let tree = tree(svg)?;
    let size = tree.size().to_int_size();
    let width = (size.width() as f32 * scale).ceil() as u32;
    let height = (size.height() as f32 * scale).ceil() as u32;
    if u64::from(width) * u64::from(height) > 100_000_000 {
        return Err(Error::Layout("raster export exceeds 100 megapixels".into()));
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| Error::Layout("cannot allocate PNG canvas".into()))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap
        .encode_png()
        .map_err(|e| Error::Layout(e.to_string()))
}
pub fn pdf(svg: &str) -> Result<Vec<u8>, Error> {
    let tree = tree(svg)?;
    svg2pdf::to_pdf(
        &tree,
        svg2pdf::ConversionOptions::default(),
        svg2pdf::PageOptions::default(),
    )
    .map_err(|e| Error::Layout(e.to_string()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_paints_resolve_without_rewriting_literal_captions() {
        let svg = crate::compile("node: \"var(--sd-fg,#233347)\"\n")
            .unwrap()
            .to_svg();
        let native = super::native_svg(&svg);
        assert!(native.contains(">var(--sd-fg,#233347)</text>"));
        assert!(native.contains("fill=\"#e5eef6\""));
        assert!(native.contains("font-size:15px;fill:#233347"));
        assert!(!native.contains("fill=\"var("));
        assert!(!native.contains("fill:var("));
    }

    #[test]
    fn themed_code_and_marching_ants_render_to_png() {
        let src = r#"
theme: catppuccin-mocha
api: "API\n```rust\npub fn handle() -> bool { true }\n```"
db: "Store\n```sql\nSELECT id FROM jobs;\n```"
api -> db: "query" [style: marching-ants]
"#;
        let svg = crate::compile(src).unwrap().to_svg();
        assert!(svg.contains("data-explicit-theme=\"catppuccin-mocha\""));
        assert!(svg.contains("marching-ants"));
        let native = super::native_svg(&svg);
        let mocha = crate::themes::by_name("catppuccin-mocha").unwrap();
        assert!(native.contains(&*mocha.bg));
        assert!(native.contains(&*mocha.syn_kw));
        let bytes =
            super::png(&svg, 1.0).expect("resvg must render themed SVG with animations & code");
        assert!(bytes.starts_with(b"\x89PNG"));
    }
}
