//! Native PNG/PDF export, optional so font/raster dependencies stay out of the core Wasm bundle.
use crate::Error;

fn color_fallbacks(value: &str) -> String {
    let mut value = value.to_owned();
    while let Some(start) = value.find("var(--sd-") {
        let Some(end) = value[start..].find(')').map(|n| start + n) else {
            break;
        };
        let Some((_, fallback)) = value[start..end].split_once(',') else {
            break;
        };
        let fallback = fallback.trim().to_owned();
        value.replace_range(start..=end, &fallback);
    }
    value
}

// Resolve only paint attributes and style declarations. A diagram caption can
// itself contain CSS syntax, which must remain literal text in native exports.
fn native_svg(svg: &str) -> String {
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
                let fallback = color_fallbacks(&tag[start..end]);
                tag.replace_range(start..end, &fallback);
                offset = start + fallback.len() + 1;
            }
        }
        output.push_str(&tag);
        cursor = end;
        if tag.starts_with("<style")
            && let Some(end) = svg[cursor..].find("</style>").map(|n| cursor + n)
        {
            output.push_str(&color_fallbacks(&svg[cursor..end]));
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
        assert!(native.contains("font-size:13px;fill:#233347"));
        assert!(!native.contains("fill=\"var("));
        assert!(!native.contains("fill:var("));
    }
}
