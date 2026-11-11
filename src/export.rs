//! Native PNG/PDF export, optional so font/raster dependencies stay out of the core Wasm bundle.
use crate::Error;

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
    resvg::usvg::Tree::from_str(svg, &options).map_err(|e| Error::Layout(e.to_string()))
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
