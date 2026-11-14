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

fn rasterize_frames(
    svgs: &[String],
    scale: f32,
) -> Result<(u32, u32, Vec<resvg::tiny_skia::Pixmap>), Error> {
    if svgs.is_empty() {
        return Err(Error::Edit(
            "animated export requires at least 1 frame".into(),
        ));
    }
    if !scale.is_finite() || scale <= 0. || scale > 8. {
        return Err(Error::Edit("scale must be in (0,8]".into()));
    }
    let mut trees = Vec::with_capacity(svgs.len());
    let (mut width, mut height) = (1u32, 1u32);
    for svg in svgs {
        let t = tree(svg)?;
        let size = t.size().to_int_size();
        width = width.max((size.width() as f32 * scale).ceil() as u32);
        height = height.max((size.height() as f32 * scale).ceil() as u32);
        trees.push(t);
    }
    if u64::from(width) * u64::from(height) > 100_000_000 {
        return Err(Error::Layout("raster export exceeds 100 megapixels".into()));
    }
    let mut pixmaps = Vec::with_capacity(trees.len());
    for t in trees {
        let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
            .ok_or_else(|| Error::Layout("cannot allocate animation frame canvas".into()))?;
        pixmap.fill(resvg::tiny_skia::Color::WHITE);
        resvg::render(
            &t,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        pixmaps.push(pixmap);
    }
    Ok((width, height, pixmaps))
}

/// Encode SVG frames into an infinitely looping Animated PNG (APNG) byte stream.
pub fn apng(svgs: &[String], scale: f32, delay_ms: u16) -> Result<Vec<u8>, Error> {
    let (width, height, pixmaps) = rasterize_frames(svgs, scale)?;
    let num_frames = pixmaps.len() as u32;
    let delay_num = delay_ms.max(20);
    let mut out = Vec::new();
    const PNG_SIG: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    out.extend_from_slice(&PNG_SIG);

    let first_png = pixmaps[0]
        .encode_png()
        .map_err(|e| Error::Layout(e.to_string()))?;
    let first_chunks = parse_png_chunks(&first_png)?;

    let mut seq = 0u32;
    let mut emitted_actl = false;
    let mut emitted_first_fctl = false;
    for (tag, data) in first_chunks {
        match &tag {
            b"IHDR" => {
                write_png_chunk(&mut out, b"IHDR", &data);
                let mut actl = Vec::with_capacity(8);
                actl.extend_from_slice(&num_frames.to_be_bytes());
                actl.extend_from_slice(&0u32.to_be_bytes());
                write_png_chunk(&mut out, b"acTL", &actl);
                emitted_actl = true;
            }
            b"IDAT" => {
                if !emitted_first_fctl {
                    let fctl = build_fctl(seq, width, height, delay_num);
                    write_png_chunk(&mut out, b"fcTL", &fctl);
                    seq += 1;
                    emitted_first_fctl = true;
                }
                write_png_chunk(&mut out, b"IDAT", &data);
            }
            b"PLTE" | b"tRNS" => {
                write_png_chunk(&mut out, &tag, &data);
            }
            b"IEND" => {}
            _ => {}
        }
    }
    if !emitted_actl {
        return Err(Error::Layout("missing IHDR chunk in encoded PNG".into()));
    }

    for pixmap in &pixmaps[1..] {
        let png_bytes = pixmap
            .encode_png()
            .map_err(|e| Error::Layout(e.to_string()))?;
        let chunks = parse_png_chunks(&png_bytes)?;
        let fctl = build_fctl(seq, width, height, delay_num);
        write_png_chunk(&mut out, b"fcTL", &fctl);
        seq += 1;
        for (tag, data) in chunks {
            if &tag == b"IDAT" {
                let mut fdat = Vec::with_capacity(4 + data.len());
                fdat.extend_from_slice(&seq.to_be_bytes());
                fdat.extend_from_slice(&data);
                write_png_chunk(&mut out, b"fdAT", &fdat);
                seq += 1;
            }
        }
    }

    write_png_chunk(&mut out, b"IEND", &[]);
    Ok(out)
}

type PngChunk = ([u8; 4], Vec<u8>);

fn parse_png_chunks(bytes: &[u8]) -> Result<Vec<PngChunk>, Error> {
    if bytes.len() < 8 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return Err(Error::Layout("invalid PNG header".into()));
    }
    let mut pos = 8;
    let mut chunks = Vec::new();
    while pos + 12 <= bytes.len() {
        let len = u32::from_be_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]])
            as usize;
        let tag = [
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ];
        let data_start = pos + 8;
        let data_end = data_start + len;
        if data_end + 4 > bytes.len() {
            return Err(Error::Layout("truncated PNG chunk".into()));
        }
        chunks.push((tag, bytes[data_start..data_end].to_vec()));
        pos = data_end + 4;
        if &tag == b"IEND" {
            break;
        }
    }
    Ok(chunks)
}

fn build_fctl(seq: u32, width: u32, height: u32, delay_ms: u16) -> Vec<u8> {
    let mut buf = Vec::with_capacity(26);
    buf.extend_from_slice(&seq.to_be_bytes());
    buf.extend_from_slice(&width.to_be_bytes());
    buf.extend_from_slice(&height.to_be_bytes());
    buf.extend_from_slice(&0u32.to_be_bytes()); // x_offset
    buf.extend_from_slice(&0u32.to_be_bytes()); // y_offset
    buf.extend_from_slice(&delay_ms.to_be_bytes());
    buf.extend_from_slice(&1000u16.to_be_bytes()); // delay_den
    buf.push(1); // dispose_op: APNG_DISPOSE_OP_BACKGROUND
    buf.push(0); // blend_op: APNG_BLEND_OP_SOURCE
    buf
}

fn write_png_chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    let crc = crc32_png(tag, data);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn crc32_png(tag: &[u8; 4], data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in tag.iter().chain(data.iter()) {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Encode SVG frames into an infinitely looping GIF89a animation using a 256-color
/// median-cut global palette and pure-Rust LZW compression.
pub fn gif(svgs: &[String], scale: f32, delay_cs: u16) -> Result<Vec<u8>, Error> {
    let (width, height, pixmaps) = rasterize_frames(svgs, scale)?;
    if width > u32::from(u16::MAX) || height > u32::from(u16::MAX) {
        return Err(Error::Layout("GIF dimensions exceed 65535px".into()));
    }
    let palette = quantize_frames_256(&pixmaps);
    let mut lut = vec![u16::MAX; 32768];
    let delay = delay_cs.max(2);

    let mut out = Vec::with_capacity(1024 + pixmaps.len() * (width * height / 4) as usize);
    out.extend_from_slice(b"GIF89a");
    out.extend_from_slice(&(width as u16).to_le_bytes());
    out.extend_from_slice(&(height as u16).to_le_bytes());
    out.push(0xF7); // GCT present, 8-bit color resolution, 256 entries (2^(7+1))
    out.push(0); // bg index
    out.push(0); // aspect
    for c in &palette {
        out.extend_from_slice(c);
    }
    // NETSCAPE2.0 infinite loop application extension
    out.extend_from_slice(&[
        0x21, 0xFF, 0x0B, b'N', b'E', b'T', b'S', b'C', b'A', b'P', b'E', b'2', b'.', b'0', 0x03,
        0x01, 0x00, 0x00, 0x00,
    ]);

    let mut indices = vec![0u8; (width * height) as usize];
    for pixmap in &pixmaps {
        for (dst, px) in indices
            .iter_mut()
            .zip(pixmap.data().as_chunks::<4>().0.iter())
        {
            let key = rgb15_key(px[0], px[1], px[2]);
            let mut idx = lut[key];
            if idx == u16::MAX {
                idx = u16::from(nearest_palette_idx(&palette, px[0], px[1], px[2]));
                lut[key] = idx;
            }
            *dst = idx as u8;
        }
        // Graphic Control Extension
        out.extend_from_slice(&[0x21, 0xF9, 0x04, 0x04]);
        out.extend_from_slice(&delay.to_le_bytes());
        out.extend_from_slice(&[0x00, 0x00]);
        // Image Descriptor
        out.push(0x2C);
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(width as u16).to_le_bytes());
        out.extend_from_slice(&(height as u16).to_le_bytes());
        out.push(0x00); // no local color table
        encode_lzw_gif(&mut out, &indices);
    }

    out.push(0x3B);
    Ok(out)
}

#[inline]
fn rgb15_key(r: u8, g: u8, b: u8) -> usize {
    (usize::from(r >> 3) << 10) | (usize::from(g >> 3) << 5) | usize::from(b >> 3)
}

#[derive(Clone, Default)]
struct HistBin {
    count: u64,
    sum_r: u64,
    sum_g: u64,
    sum_b: u64,
    avg: [u8; 3],
}

fn quantize_frames_256(pixmaps: &[resvg::tiny_skia::Pixmap]) -> Vec<[u8; 3]> {
    let mut bins = vec![HistBin::default(); 32768];
    for pm in pixmaps {
        for px in pm.data().as_chunks::<4>().0 {
            let idx = rgb15_key(px[0], px[1], px[2]);
            let bin = &mut bins[idx];
            bin.count += 1;
            bin.sum_r += u64::from(px[0]);
            bin.sum_g += u64::from(px[1]);
            bin.sum_b += u64::from(px[2]);
        }
    }
    let mut active: Vec<HistBin> = bins
        .into_iter()
        .filter(|b| b.count > 0)
        .map(|mut b| {
            b.avg = [
                (b.sum_r / b.count) as u8,
                (b.sum_g / b.count) as u8,
                (b.sum_b / b.count) as u8,
            ];
            b
        })
        .collect();
    if active.is_empty() {
        return vec![[255, 255, 255]; 256];
    }

    let mut boxes: Vec<(usize, usize)> = vec![(0, active.len())];
    while boxes.len() < 256 {
        let mut best_box = None;
        let mut best_span = 0u8;
        for (bi, &(s, e)) in boxes.iter().enumerate() {
            if e - s <= 1 {
                continue;
            }
            let span = box_max_span(&active[s..e]).3;
            if span > best_span {
                best_span = span;
                best_box = Some(bi);
            }
        }
        let Some(bi) = best_box else {
            break;
        };
        let (s, e) = boxes.swap_remove(bi);
        let (ch, _, _, _) = box_max_span(&active[s..e]);
        active[s..e].sort_by_key(|b| b.avg[ch]);
        let total: u64 = active[s..e].iter().map(|b| b.count).sum();
        let mut acc = 0u64;
        let mut mid = s + 1;
        for (idx, item) in active.iter().enumerate().take(e.saturating_sub(1)).skip(s) {
            acc += item.count;
            if acc * 2 >= total {
                mid = (idx + 1).max(s + 1);
                break;
            }
        }
        boxes.push((s, mid));
        boxes.push((mid, e));
    }

    let mut palette: Vec<[u8; 3]> = boxes
        .into_iter()
        .map(|(s, e)| {
            let (mut sr, mut sg, mut sb, mut cnt) = (0u64, 0u64, 0u64, 0u64);
            for b in &active[s..e] {
                sr += b.sum_r;
                sg += b.sum_g;
                sb += b.sum_b;
                cnt += b.count;
            }
            let cnt = cnt.max(1);
            [(sr / cnt) as u8, (sg / cnt) as u8, (sb / cnt) as u8]
        })
        .collect();
    palette.resize(256, [255, 255, 255]);
    palette
}

fn box_max_span(slice: &[HistBin]) -> (usize, u8, u8, u8) {
    let (mut min, mut max) = ([255u8; 3], [0u8; 3]);
    for b in slice {
        for c in 0..3 {
            min[c] = min[c].min(b.avg[c]);
            max[c] = max[c].max(b.avg[c]);
        }
    }
    let spans = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    let mut best_c = 0;
    if spans[1] > spans[best_c] {
        best_c = 1;
    }
    if spans[2] > spans[best_c] {
        best_c = 2;
    }
    (best_c, min[best_c], max[best_c], spans[best_c])
}

fn nearest_palette_idx(palette: &[[u8; 3]], r: u8, g: u8, b: u8) -> u8 {
    let mut best_i = 0u8;
    let mut best_d = u32::MAX;
    for (i, c) in palette.iter().enumerate() {
        let dr = i32::from(r) - i32::from(c[0]);
        let dg = i32::from(g) - i32::from(c[1]);
        let db = i32::from(b) - i32::from(c[2]);
        let d = (dr * dr + dg * dg + db * db) as u32;
        if d < best_d {
            best_d = d;
            best_i = i as u8;
            if d == 0 {
                break;
            }
        }
    }
    best_i
}

fn encode_lzw_gif(out: &mut Vec<u8>, indices: &[u8]) {
    out.push(8); // min LZW code size = 8 bits
    if indices.is_empty() {
        out.extend_from_slice(&[3, 0x00, 0x01, 0x02, 0]);
        return;
    }

    let mut payload = Vec::with_capacity(indices.len() / 2);
    let mut bit_buf = 0u32;
    let mut bit_cnt = 0u8;
    let mut push_code = |code: u16, bits: u8, dest: &mut Vec<u8>| {
        bit_buf |= u32::from(code) << bit_cnt;
        bit_cnt += bits;
        while bit_cnt >= 8 {
            dest.push(bit_buf as u8);
            bit_buf >>= 8;
            bit_cnt -= 8;
        }
    };

    const TABLE_SZ: usize = 16384;
    const CLEAR: u16 = 256;
    const EOI: u16 = 257;
    let mut keys = vec![u32::MAX; TABLE_SZ];
    let mut vals = vec![0u16; TABLE_SZ];

    let mut next_code = 258u16;
    let mut bits = 9u8;
    push_code(CLEAR, bits, &mut payload);

    let mut prefix = u16::from(indices[0]);
    for &sym in &indices[1..] {
        let key = (u32::from(prefix) << 8) | u32::from(sym);
        let mut slot = ((key.wrapping_mul(2_654_435_761)) >> 18) as usize & (TABLE_SZ - 1);
        let mut found = None;
        while keys[slot] != u32::MAX {
            if keys[slot] == key {
                found = Some(vals[slot]);
                break;
            }
            slot = (slot + 1) & (TABLE_SZ - 1);
        }
        if let Some(code) = found {
            prefix = code;
        } else {
            push_code(prefix, bits, &mut payload);
            if next_code < 4096 {
                keys[slot] = key;
                vals[slot] = next_code;
                next_code += 1;
                if next_code > (1u16 << bits) && bits < 12 {
                    bits += 1;
                }
            } else {
                push_code(CLEAR, bits, &mut payload);
                keys.fill(u32::MAX);
                next_code = 258;
                bits = 9;
            }
            prefix = u16::from(sym);
        }
    }

    push_code(prefix, bits, &mut payload);
    push_code(EOI, bits, &mut payload);
    if bit_cnt > 0 {
        payload.push(bit_buf as u8);
    }

    for chunk in payload.chunks(255) {
        out.push(chunk.len() as u8);
        out.extend_from_slice(chunk);
    }
    out.push(0); // block terminator
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

    #[test]
    fn encodes_valid_multiframe_apng_and_gif() {
        let f1 = crate::compile("a -> b: step 1\n").unwrap().to_svg();
        let f2 = crate::compile("a -> b: step 2\nb [label: \"Active\"]\n")
            .unwrap()
            .to_svg();
        let frames = vec![f1, f2];

        let apng_bytes = super::apng(&frames, 1.0, 250).unwrap();
        assert!(apng_bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(apng_bytes.windows(4).any(|w| w == b"acTL"));
        assert!(apng_bytes.windows(4).any(|w| w == b"fcTL"));
        assert!(apng_bytes.windows(4).any(|w| w == b"fdAT"));

        let gif_bytes = super::gif(&frames, 1.0, 25).unwrap();
        assert!(gif_bytes.starts_with(b"GIF89a"));
        assert!(gif_bytes.windows(11).any(|w| w == b"NETSCAPE2.0"));
        assert_eq!(gif_bytes.last().copied(), Some(0x3B));
    }
}
