//! Curated color palettes and Base16 scheme loading with WCAG contrast guarantees.
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, collections::BTreeMap, path::Path};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemePalette {
    pub id: Cow<'static, str>,
    pub label: Cow<'static, str>,
    pub dark: bool,
    pub bg: Cow<'static, str>,
    pub surface: Cow<'static, str>,
    pub fg: Cow<'static, str>,
    pub edge: Cow<'static, str>,
    pub muted: Cow<'static, str>,
    pub node: Cow<'static, str>,
    pub border: Cow<'static, str>,
    pub database: Cow<'static, str>,
    pub cap: Cow<'static, str>,
    pub database_border: Cow<'static, str>,
    pub actor: Cow<'static, str>,
    pub actor_border: Cow<'static, str>,
    pub cache: Cow<'static, str>,
    pub cache_border: Cow<'static, str>,
    pub decision: Cow<'static, str>,
    pub decision_border: Cow<'static, str>,
    pub queue: Cow<'static, str>,
    pub queue_line: Cow<'static, str>,
    pub container: Cow<'static, str>,
    pub plane: Cow<'static, str>,
    pub zone_text: Cow<'static, str>,
    pub flow: Cow<'static, str>,
    pub flow_text: Cow<'static, str>,
    pub story: Cow<'static, str>,
    pub added: Cow<'static, str>,
    pub removed: Cow<'static, str>,
    pub modified: Cow<'static, str>,
    pub status: Cow<'static, str>,
    pub overload: Cow<'static, str>,
    pub overload_fill: Cow<'static, str>,
    pub cut: Cow<'static, str>,
    pub code_bg: Cow<'static, str>,
    pub code_border: Cow<'static, str>,
    pub code_header: Cow<'static, str>,
    pub syn_kw: Cow<'static, str>,
    pub syn_str: Cow<'static, str>,
    pub syn_num: Cow<'static, str>,
    pub syn_cmt: Cow<'static, str>,
    pub syn_type: Cow<'static, str>,
    pub syn_fn: Cow<'static, str>,
}

impl ThemePalette {
    /// Emit inline CSS custom properties `--sd-*:value;` matching `web/diagram-palette.css`.
    pub fn css_vars(&self) -> String {
        format!(
            "--sd-bg:{};--sd-surface:{};--sd-fg:{};--sd-edge:{};--sd-muted:{};--sd-node:{};--sd-border:{};--sd-database:{};--sd-cap:{};--sd-database-border:{};--sd-actor:{};--sd-actor-border:{};--sd-cache:{};--sd-cache-border:{};--sd-decision:{};--sd-decision-border:{};--sd-queue:{};--sd-queue-line:{};--sd-container:{};--sd-plane:{};--sd-zone-text:{};--sd-flow:{};--sd-flow-text:{};--sd-story:{};--sd-added:{};--sd-removed:{};--sd-modified:{};--sd-status:{};--sd-overload:{};--sd-overload-fill:{};--sd-cut:{};--sd-code-bg:{};--sd-code-border:{};--sd-code-header:{};--sd-syn-kw:{};--sd-syn-str:{};--sd-syn-num:{};--sd-syn-cmt:{};--sd-syn-type:{};--sd-syn-fn:{}",
            self.bg,
            self.surface,
            self.fg,
            self.edge,
            self.muted,
            self.node,
            self.border,
            self.database,
            self.cap,
            self.database_border,
            self.actor,
            self.actor_border,
            self.cache,
            self.cache_border,
            self.decision,
            self.decision_border,
            self.queue,
            self.queue_line,
            self.container,
            self.plane,
            self.zone_text,
            self.flow,
            self.flow_text,
            self.story,
            self.added,
            self.removed,
            self.modified,
            self.status,
            self.overload,
            self.overload_fill,
            self.cut,
            self.code_bg,
            self.code_border,
            self.code_header,
            self.syn_kw,
            self.syn_str,
            self.syn_num,
            self.syn_cmt,
            self.syn_type,
            self.syn_fn,
        )
    }

    /// Parse a Base16 scheme specification (standard YAML `base00..base0F`, tinted-theming `palette:`, or JSON).
    pub fn from_base16_str(spec: &str) -> Result<Self, crate::Error> {
        let mut scheme: Option<String> = None;
        let mut variant: Option<bool> = None;
        let mut raw = BTreeMap::<String, Rgb>::new();

        if let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(spec)
        {
            if let Some(s) = map.get("scheme").and_then(|v| v.as_str()) {
                scheme = Some(s.to_string());
            } else if let Some(s) = map.get("name").and_then(|v| v.as_str()) {
                scheme = Some(s.to_string());
            }
            if let Some(v) = map.get("variant").and_then(|v| v.as_str()) {
                variant = match v.to_ascii_lowercase().as_str() {
                    "dark" => Some(true),
                    "light" => Some(false),
                    _ => None,
                };
            }
            let palette_map = map
                .get("palette")
                .and_then(|v| v.as_object())
                .unwrap_or(&map);
            for (k, v) in palette_map {
                let key = k.trim().to_ascii_lowercase();
                if key.starts_with("base0")
                    && key.len() == 6
                    && let Some(hex) = v.as_str().and_then(parse_hex)
                {
                    raw.insert(key, hex);
                }
            }
        } else {
            for line in spec.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.ends_with(':') {
                    continue;
                }
                let Some((k, rest)) = trimmed.split_once(':') else {
                    continue;
                };
                let key = k
                    .trim()
                    .trim_matches(|c| matches!(c, '"' | '\''))
                    .to_ascii_lowercase();
                let val = strip_yaml_scalar(rest);
                if key == "scheme" || key == "name" {
                    if !val.is_empty() {
                        scheme = Some(val.to_string());
                    }
                } else if key == "variant" {
                    variant = match val.to_ascii_lowercase().as_str() {
                        "dark" => Some(true),
                        "light" => Some(false),
                        _ => None,
                    };
                } else if key.starts_with("base0")
                    && key.len() == 6
                    && let Some(hex) = parse_hex(val)
                {
                    raw.insert(key, hex);
                }
            }
        }

        let keys = [
            "base00", "base01", "base02", "base03", "base04", "base05", "base06", "base07",
            "base08", "base09", "base0a", "base0b", "base0c", "base0d", "base0e", "base0f",
        ];
        let mut slots = [Rgb(0, 0, 0); 16];
        for (idx, k) in keys.iter().enumerate() {
            slots[idx] = *raw
                .get(*k)
                .ok_or_else(|| crate::Error::Layout(format!("Base16 scheme missing slot {k}")))?;
        }
        let label = scheme.unwrap_or_else(|| "Base16 Custom".to_string());
        let id = format!("base16-{}", slugify(&label));
        Ok(from_base16_slots(id, label, variant, slots))
    }

    /// Load every Base16 YAML/JSON scheme file from a directory into a sorted family list.
    pub fn from_base16_dir(dir: &Path) -> Result<Vec<Self>, crate::Error> {
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .map_err(|e| {
                crate::Error::Layout(format!("cannot read theme dir {}: {e}", dir.display()))
            })?
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|p| {
                p.is_file()
                    && p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                        matches!(e.to_ascii_lowercase().as_str(), "yaml" | "yml" | "json")
                    })
            })
            .collect();
        files.sort();
        let mut out = Vec::with_capacity(files.len());
        for path in files {
            let content = std::fs::read_to_string(&path).map_err(|e| {
                crate::Error::Layout(format!("cannot read theme file {}: {e}", path.display()))
            })?;
            out.push(Self::from_base16_str(&content)?);
        }
        Ok(out)
    }
}

fn strip_yaml_scalar(s: &str) -> &str {
    let s = s.trim();
    if let Some(inner) = s.strip_prefix('"')
        && let Some(end) = inner.find('"')
    {
        return inner[..end].trim();
    }
    if let Some(inner) = s.strip_prefix('\'')
        && let Some(end) = inner.find('\'')
    {
        return inner[..end].trim();
    }
    if let Some((head, _)) = s.split_once('#') {
        let head = head.trim();
        if !head.is_empty() {
            return head;
        }
    }
    s.trim()
}

fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_end_matches('-').to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rgb(u8, u8, u8);

impl Rgb {
    fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

fn parse_hex(s: &str) -> Option<Rgb> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some(Rgb(r, g, b))
}

fn must_hex(s: &str) -> Rgb {
    parse_hex(s).expect("valid static hex color")
}

fn luminance(c: Rgb) -> f64 {
    let chan = |u: u8| {
        let v = u as f64 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * chan(c.0) + 0.7152 * chan(c.1) + 0.0722 * chan(c.2)
}

fn contrast(a: Rgb, b: Rgb) -> f64 {
    let la = luminance(a);
    let lb = luminance(b);
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let ch = |x: u8, y: u8| -> u8 { ((x as f64) * (1.0 - t) + (y as f64) * t).round() as u8 };
    Rgb(ch(a.0, b.0), ch(a.1, b.1), ch(a.2, b.2))
}

/// Nudge `color` toward `target` (white or dark ink) in small steps until `contrast >= min_ratio`.
fn ensure_contrast_against(color: Rgb, bg: Rgb, min_ratio: f64, prefer_light: bool) -> Rgb {
    if contrast(color, bg) >= min_ratio {
        return color;
    }
    let primary = if prefer_light {
        Rgb(255, 255, 255)
    } else {
        Rgb(8, 12, 18)
    };
    let secondary = if prefer_light {
        Rgb(8, 12, 18)
    } else {
        Rgb(255, 255, 255)
    };
    for pole in [primary, secondary] {
        if contrast(pole, bg) < min_ratio {
            continue;
        }
        let mut lo = 0.0;
        let mut hi = 1.0;
        for _ in 0..24 {
            let mid = (lo + hi) * 0.5;
            if contrast(mix(color, pole, mid), bg) >= min_ratio {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        return mix(color, pole, hi);
    }
    primary
}

/// Pull a tinted shape fill away from `fg` if needed so `fg` meets `4.65:1` contrast,
/// then adjust `border` so it meets `3.15:1` against the final fill.
fn shape_pair(
    raw_fill: Rgb,
    raw_border: Rgb,
    bg: Rgb,
    fg: Rgb,
    dark: bool,
) -> (Cow<'static, str>, Cow<'static, str>) {
    let mut fill = raw_fill;
    if contrast(fg, fill) < 4.65 {
        let safe_pole = if dark {
            mix(bg, Rgb(0, 0, 0), 0.35)
        } else {
            Rgb(252, 253, 255)
        };
        let mut lo = 0.0;
        let mut hi = 1.0;
        for _ in 0..24 {
            let mid = (lo + hi) * 0.5;
            if contrast(fg, mix(fill, safe_pole, mid)) >= 4.65 {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        fill = mix(fill, safe_pole, hi);
    }
    let border = ensure_contrast_against(raw_border, fill, 3.15, dark);
    (Cow::Owned(fill.hex()), Cow::Owned(border.hex()))
}

fn from_base16_slots(
    id: String,
    label: String,
    variant: Option<bool>,
    b: [Rgb; 16],
) -> ThemePalette {
    let dark = variant.unwrap_or_else(|| luminance(b[0]) < 0.22);
    let bg = b[0];
    let surface = mix(b[0], b[1], 0.72);
    let fg = ensure_contrast_against(
        ensure_contrast_against(b[5], bg, 6.2, dark),
        surface,
        5.0,
        dark,
    );
    let fill_weight = if dark { 0.23 } else { 0.14 };
    let (node, border) = shape_pair(mix(bg, b[13], fill_weight), b[13], bg, fg, dark);
    let (database, database_border) = shape_pair(mix(bg, b[12], fill_weight), b[12], bg, fg, dark);
    let cap_rgb = mix(must_hex(&database), b[12], if dark { 0.18 } else { 0.14 });
    let (actor, actor_border) = shape_pair(mix(bg, b[11], fill_weight), b[11], bg, fg, dark);
    let (cache, cache_border) = shape_pair(mix(bg, b[9], fill_weight), b[9], bg, fg, dark);
    let (decision, decision_border) = shape_pair(mix(bg, b[10], fill_weight), b[10], bg, fg, dark);
    let (queue, queue_line) = shape_pair(mix(bg, b[14], fill_weight), b[14], bg, fg, dark);
    let border = ensure_contrast_against(
        ensure_contrast_against(must_hex(&border), surface, 3.15, dark),
        must_hex(&queue),
        3.15,
        dark,
    );

    let edge = ensure_contrast_against(mix(b[4], fg, 0.35), bg, 4.55, dark);
    let muted = ensure_contrast_against(b[3], bg, 3.1, dark);
    let container = mix(bg, b[1], if dark { 0.75 } else { 0.65 });
    let plane = mix(bg, b[2], if dark { 0.55 } else { 0.45 });
    let zone_text = ensure_contrast_against(b[12], plane, 4.6, dark);
    let flow = b[13];
    let flow_text = if luminance(flow) < 0.28 {
        Rgb(255, 255, 255)
    } else {
        Rgb(16, 24, 34)
    };
    let story = ensure_contrast_against(b[13], bg, 4.5, dark);
    let overload_fill = mix(bg, b[8], if dark { 0.26 } else { 0.16 });
    let overload = ensure_contrast_against(b[8], overload_fill, 4.6, dark);
    let cut = ensure_contrast_against(b[8], bg, 4.5, dark);

    let code_bg = if dark {
        mix(bg, Rgb(6, 8, 12), 0.30)
    } else {
        mix(bg, b[1], 0.85)
    };
    let code_border = ensure_contrast_against(b[3], code_bg, 2.2, dark);
    let code_header = mix(code_bg, b[2], 0.45);
    let syn_kw = ensure_contrast_against(b[14], code_bg, 4.6, dark);
    let syn_str = ensure_contrast_against(b[11], code_bg, 4.6, dark);
    let syn_num = ensure_contrast_against(b[9], code_bg, 4.6, dark);
    let syn_cmt = ensure_contrast_against(mix(b[3], b[4], 0.45), code_bg, 4.6, dark);
    let syn_type = ensure_contrast_against(b[10], code_bg, 4.6, dark);
    let syn_fn = ensure_contrast_against(b[13], code_bg, 4.6, dark);

    ThemePalette {
        id: Cow::Owned(id),
        label: Cow::Owned(label),
        dark,
        bg: Cow::Owned(bg.hex()),
        surface: Cow::Owned(surface.hex()),
        fg: Cow::Owned(fg.hex()),
        edge: Cow::Owned(edge.hex()),
        muted: Cow::Owned(muted.hex()),
        node,
        border: Cow::Owned(border.hex()),
        database,
        cap: Cow::Owned(cap_rgb.hex()),
        database_border,
        actor,
        actor_border,
        cache,
        cache_border,
        decision,
        decision_border,
        queue,
        queue_line,
        container: Cow::Owned(container.hex()),
        plane: Cow::Owned(plane.hex()),
        zone_text: Cow::Owned(zone_text.hex()),
        flow: Cow::Owned(flow.hex()),
        flow_text: Cow::Owned(flow_text.hex()),
        story: Cow::Owned(story.hex()),
        added: Cow::Owned(ensure_contrast_against(b[11], bg, 3.8, dark).hex()),
        removed: Cow::Owned(ensure_contrast_against(b[9], bg, 3.8, dark).hex()),
        modified: Cow::Owned(ensure_contrast_against(b[14], bg, 3.8, dark).hex()),
        status: Cow::Owned(ensure_contrast_against(b[12], bg, 4.55, dark).hex()),
        overload: Cow::Owned(overload.hex()),
        overload_fill: Cow::Owned(overload_fill.hex()),
        cut: Cow::Owned(cut.hex()),
        code_bg: Cow::Owned(code_bg.hex()),
        code_border: Cow::Owned(code_border.hex()),
        code_header: Cow::Owned(code_header.hex()),
        syn_kw: Cow::Owned(syn_kw.hex()),
        syn_str: Cow::Owned(syn_str.hex()),
        syn_num: Cow::Owned(syn_num.hex()),
        syn_cmt: Cow::Owned(syn_cmt.hex()),
        syn_type: Cow::Owned(syn_type.hex()),
        syn_fn: Cow::Owned(syn_fn.hex()),
    }
}

#[allow(clippy::too_many_arguments)]
fn curated(
    id: &'static str,
    label: &'static str,
    dark: bool,
    bg_hex: &str,
    surface_hex: &str,
    fg_hex: &str,
    edge_hex: &str,
    muted_hex: &str,
    blue: Rgb,
    sapphire: Rgb,
    green: Rgb,
    peach: Rgb,
    yellow: Rgb,
    mauve: Rgb,
    red: Rgb,
    teal: Rgb,
    code_bg_hex: &str,
) -> ThemePalette {
    let bg = must_hex(bg_hex);
    let surface = must_hex(surface_hex);
    let fg = ensure_contrast_against(
        ensure_contrast_against(must_hex(fg_hex), bg, 5.2, dark),
        surface,
        4.8,
        dark,
    );
    let w = if dark { 0.22 } else { 0.14 };
    let (node, border) = shape_pair(mix(bg, blue, w), blue, bg, fg, dark);
    let (database, database_border) = shape_pair(mix(bg, sapphire, w), sapphire, bg, fg, dark);
    let cap_rgb = mix(
        must_hex(&database),
        sapphire,
        if dark { 0.18 } else { 0.13 },
    );
    let (actor, actor_border) = shape_pair(mix(bg, green, w), green, bg, fg, dark);
    let (cache, cache_border) = shape_pair(mix(bg, peach, w), peach, bg, fg, dark);
    let (decision, decision_border) = shape_pair(mix(bg, yellow, w), yellow, bg, fg, dark);
    let (queue, queue_line) = shape_pair(mix(bg, mauve, w), mauve, bg, fg, dark);
    let border = ensure_contrast_against(
        ensure_contrast_against(must_hex(&border), surface, 3.15, dark),
        must_hex(&queue),
        3.15,
        dark,
    );

    let container = mix(bg, surface, 0.72);
    let plane = mix(bg, surface, 0.92);
    let zone_text = ensure_contrast_against(teal, plane, 4.6, dark);
    let edge = ensure_contrast_against(must_hex(edge_hex), bg, 4.55, dark);
    let muted = ensure_contrast_against(must_hex(muted_hex), bg, 3.1, dark);
    let flow_text = if luminance(blue) < 0.28 {
        Rgb(255, 255, 255)
    } else {
        Rgb(20, 26, 36)
    };
    let code_bg = must_hex(code_bg_hex);
    let code_border = ensure_contrast_against(must_hex(muted_hex), code_bg, 2.2, dark);
    let code_header = mix(code_bg, surface, 0.5);
    let overload_fill = mix(bg, red, if dark { 0.25 } else { 0.15 });

    ThemePalette {
        id: Cow::Borrowed(id),
        label: Cow::Borrowed(label),
        dark,
        bg: Cow::Owned(bg.hex()),
        surface: Cow::Owned(surface.hex()),
        fg: Cow::Owned(fg.hex()),
        edge: Cow::Owned(edge.hex()),
        muted: Cow::Owned(muted.hex()),
        node,
        border: Cow::Owned(border.hex()),
        database,
        cap: Cow::Owned(cap_rgb.hex()),
        database_border,
        actor,
        actor_border,
        cache,
        cache_border,
        decision,
        decision_border,
        queue,
        queue_line,
        container: Cow::Owned(container.hex()),
        plane: Cow::Owned(plane.hex()),
        zone_text: Cow::Owned(zone_text.hex()),
        flow: Cow::Owned(blue.hex()),
        flow_text: Cow::Owned(flow_text.hex()),
        story: Cow::Owned(ensure_contrast_against(blue, bg, 4.5, dark).hex()),
        added: Cow::Owned(ensure_contrast_against(green, bg, 3.8, dark).hex()),
        removed: Cow::Owned(ensure_contrast_against(yellow, bg, 3.8, dark).hex()),
        modified: Cow::Owned(ensure_contrast_against(mauve, bg, 3.8, dark).hex()),
        status: Cow::Owned(ensure_contrast_against(teal, bg, 4.55, dark).hex()),
        overload: Cow::Owned(ensure_contrast_against(red, overload_fill, 4.6, dark).hex()),
        overload_fill: Cow::Owned(overload_fill.hex()),
        cut: Cow::Owned(ensure_contrast_against(red, bg, 4.55, dark).hex()),
        code_bg: Cow::Owned(code_bg.hex()),
        code_border: Cow::Owned(code_border.hex()),
        code_header: Cow::Owned(code_header.hex()),
        syn_kw: Cow::Owned(ensure_contrast_against(mauve, code_bg, 4.6, dark).hex()),
        syn_str: Cow::Owned(ensure_contrast_against(green, code_bg, 4.6, dark).hex()),
        syn_num: Cow::Owned(ensure_contrast_against(peach, code_bg, 4.6, dark).hex()),
        syn_cmt: Cow::Owned(ensure_contrast_against(must_hex(muted_hex), code_bg, 4.6, dark).hex()),
        syn_type: Cow::Owned(ensure_contrast_against(yellow, code_bg, 4.6, dark).hex()),
        syn_fn: Cow::Owned(ensure_contrast_against(blue, code_bg, 4.6, dark).hex()),
    }
}

pub fn default_light() -> ThemePalette {
    ThemePalette {
        id: Cow::Borrowed("light"),
        label: Cow::Borrowed("Smarter Light"),
        dark: false,
        bg: Cow::Borrowed("#ffffff"),
        surface: Cow::Borrowed("#f6f8fb"),
        fg: Cow::Borrowed("#233347"),
        edge: Cow::Borrowed("#53657a"),
        muted: Cow::Borrowed("#73869b"),
        node: Cow::Borrowed("#e5eef6"),
        border: Cow::Borrowed("#6b8198"),
        database: Cow::Borrowed("#e1edf9"),
        cap: Cow::Borrowed("#d1e3f5"),
        database_border: Cow::Borrowed("#6483a2"),
        actor: Cow::Borrowed("#e4f2ea"),
        actor_border: Cow::Borrowed("#608b75"),
        cache: Cow::Borrowed("#fff1d6"),
        cache_border: Cow::Borrowed("#95723b"),
        decision: Cow::Borrowed("#fff0d8"),
        decision_border: Cow::Borrowed("#98702e"),
        queue: Cow::Borrowed("#e7e0fa"),
        queue_line: Cow::Borrowed("#b4a5d4"),
        container: Cow::Borrowed("#f0f4f9"),
        plane: Cow::Borrowed("#edf3fa"),
        zone_text: Cow::Borrowed("#29493f"),
        flow: Cow::Borrowed("#356bd9"),
        flow_text: Cow::Borrowed("#ffffff"),
        story: Cow::Borrowed("#3f6dc5"),
        added: Cow::Borrowed("#29966f"),
        removed: Cow::Borrowed("#c58b22"),
        modified: Cow::Borrowed("#6276bd"),
        status: Cow::Borrowed("#176e62"),
        overload: Cow::Borrowed("#a53b24"),
        overload_fill: Cow::Borrowed("#fff0e6"),
        cut: Cow::Borrowed("#b33336"),
        code_bg: Cow::Borrowed("#f1f5fa"),
        code_border: Cow::Borrowed("#c2d0e0"),
        code_header: Cow::Borrowed("#e4ecf5"),
        syn_kw: Cow::Borrowed("#6d28d9"),
        syn_str: Cow::Borrowed("#047857"),
        syn_num: Cow::Borrowed("#b45309"),
        syn_cmt: Cow::Borrowed("#526378"),
        syn_type: Cow::Borrowed("#0f766e"),
        syn_fn: Cow::Borrowed("#1d4ed8"),
    }
}

pub fn default_dark() -> ThemePalette {
    ThemePalette {
        id: Cow::Borrowed("dark"),
        label: Cow::Borrowed("Smarter Dark"),
        dark: true,
        bg: Cow::Borrowed("#1b2029"),
        surface: Cow::Borrowed("#151a22"),
        fg: Cow::Borrowed("#e3eaf4"),
        edge: Cow::Borrowed("#b1c0d3"),
        muted: Cow::Borrowed("#92a6bf"),
        node: Cow::Borrowed("#27384b"),
        border: Cow::Borrowed("#91abc7"),
        database: Cow::Borrowed("#263c56"),
        cap: Cow::Borrowed("#324e6d"),
        database_border: Cow::Borrowed("#92b6dc"),
        actor: Cow::Borrowed("#233e34"),
        actor_border: Cow::Borrowed("#84bca2"),
        cache: Cow::Borrowed("#443721"),
        cache_border: Cow::Borrowed("#d6b573"),
        decision: Cow::Borrowed("#433521"),
        decision_border: Cow::Borrowed("#dbb875"),
        queue: Cow::Borrowed("#39304f"),
        queue_line: Cow::Borrowed("#b7a3de"),
        container: Cow::Borrowed("#242d3a"),
        plane: Cow::Borrowed("#263344"),
        zone_text: Cow::Borrowed("#b8e7d5"),
        flow: Cow::Borrowed("#9eb9ff"),
        flow_text: Cow::Borrowed("#172333"),
        story: Cow::Borrowed("#9eb9ff"),
        added: Cow::Borrowed("#69d4ac"),
        removed: Cow::Borrowed("#f2bc5c"),
        modified: Cow::Borrowed("#a2b4ff"),
        status: Cow::Borrowed("#78d9be"),
        overload: Cow::Borrowed("#ffb198"),
        overload_fill: Cow::Borrowed("#492b25"),
        cut: Cow::Borrowed("#ff9c9f"),
        code_bg: Cow::Borrowed("#11161e"),
        code_border: Cow::Borrowed("#39485d"),
        code_header: Cow::Borrowed("#1b2330"),
        syn_kw: Cow::Borrowed("#c4b5fd"),
        syn_str: Cow::Borrowed("#6ee7b7"),
        syn_num: Cow::Borrowed("#fdba74"),
        syn_cmt: Cow::Borrowed("#94a3b8"),
        syn_type: Cow::Borrowed("#67e8f9"),
        syn_fn: Cow::Borrowed("#93c5fd"),
    }
}

fn base16_preset(id: &str) -> Option<ThemePalette> {
    let spec = match id {
        "base16-eighties" => {
            "scheme: \"Eighties\"\nvariant: dark\nbase00: \"2d2d2d\"\nbase01: \"393939\"\nbase02: \"515151\"\nbase03: \"747369\"\nbase04: \"a09f93\"\nbase05: \"d3d0c8\"\nbase06: \"e8e6df\"\nbase07: \"f2f0ec\"\nbase08: \"f2777a\"\nbase09: \"f99157\"\nbase0a: \"ffcc66\"\nbase0b: \"99cc99\"\nbase0c: \"66cccc\"\nbase0d: \"6699cc\"\nbase0e: \"cc99cc\"\nbase0f: \"d27b53\""
        }
        "base16-ocean" => {
            "scheme: \"Ocean\"\nvariant: dark\nbase00: \"2b303b\"\nbase01: \"343d46\"\nbase02: \"4f5b66\"\nbase03: \"65737e\"\nbase04: \"a7adba\"\nbase05: \"c0c5ce\"\nbase06: \"dfe1e8\"\nbase07: \"eff1f5\"\nbase08: \"bf616a\"\nbase09: \"d08770\"\nbase0a: \"ebcb8b\"\nbase0b: \"a3be8c\"\nbase0c: \"96b5b4\"\nbase0d: \"8fa1b3\"\nbase0e: \"b48ead\"\nbase0f: \"ab7967\""
        }
        "base16-gruvbox-dark" => {
            "scheme: \"Gruvbox Dark\"\nvariant: dark\nbase00: \"282828\"\nbase01: \"3c3836\"\nbase02: \"504945\"\nbase03: \"665c54\"\nbase04: \"bdae93\"\nbase05: \"d5c4a1\"\nbase06: \"ebdbb2\"\nbase07: \"fbf1c7\"\nbase08: \"fb4934\"\nbase09: \"fe8019\"\nbase0a: \"fabd2f\"\nbase0b: \"b8bb26\"\nbase0c: \"8ec07c\"\nbase0d: \"83a598\"\nbase0e: \"d3869b\"\nbase0f: \"d65d0e\""
        }
        "base16-nord" => {
            "scheme: \"Nord\"\nvariant: dark\nbase00: \"2e3440\"\nbase01: \"3b4252\"\nbase02: \"434c5e\"\nbase03: \"4c566a\"\nbase04: \"d8dee9\"\nbase05: \"e5e9f0\"\nbase06: \"eceff4\"\nbase07: \"8fbcbb\"\nbase08: \"bf616a\"\nbase09: \"d08770\"\nbase0a: \"ebcb8b\"\nbase0b: \"a3be8c\"\nbase0c: \"88c0d0\"\nbase0d: \"81a1c1\"\nbase0e: \"b48ead\"\nbase0f: \"5e81ac\""
        }
        "base16-onedark" => {
            "scheme: \"OneDark\"\nvariant: dark\nbase00: \"282c34\"\nbase01: \"353b45\"\nbase02: \"3e4451\"\nbase03: \"545862\"\nbase04: \"565c64\"\nbase05: \"abb2bf\"\nbase06: \"b6bdca\"\nbase07: \"c8ccd4\"\nbase08: \"e06c75\"\nbase09: \"d19a66\"\nbase0a: \"e5c07b\"\nbase0b: \"98c379\"\nbase0c: \"56b6c2\"\nbase0d: \"61afef\"\nbase0e: \"c678dd\"\nbase0f: \"be5046\""
        }
        "base16-dracula" => {
            "scheme: \"Dracula\"\nvariant: dark\nbase00: \"282a36\"\nbase01: \"363447\"\nbase02: \"44475a\"\nbase03: \"6272a4\"\nbase04: \"9ea8c7\"\nbase05: \"f8f8f2\"\nbase06: \"f0f1f4\"\nbase07: \"ffffff\"\nbase08: \"ff5555\"\nbase09: \"ffb86c\"\nbase0a: \"f1fa8c\"\nbase0b: \"50fa7b\"\nbase0c: \"8be9fd\"\nbase0d: \"80bfff\"\nbase0e: \"ff79c6\"\nbase0f: \"bd93f9\""
        }
        "base16-tokyo-night" => {
            "scheme: \"Tokyo Night\"\nvariant: dark\nbase00: \"1a1b26\"\nbase01: \"16161e\"\nbase02: \"2f3549\"\nbase03: \"444b6a\"\nbase04: \"787c99\"\nbase05: \"a9b1d6\"\nbase06: \"cbccd1\"\nbase07: \"d5d6db\"\nbase08: \"c0caf5\"\nbase09: \"a9b1d6\"\nbase0a: \"0db9d7\"\nbase0b: \"9ece6a\"\nbase0c: \"b4f9f8\"\nbase0d: \"7aa2f7\"\nbase0e: \"bb9af7\"\nbase0f: \"f7768e\""
        }
        "base16-github" => {
            "scheme: \"GitHub\"\nvariant: light\nbase00: \"ffffff\"\nbase01: \"f5f5f5\"\nbase02: \"c8c8fa\"\nbase03: \"969896\"\nbase04: \"e8e8e8\"\nbase05: \"333333\"\nbase06: \"ffffff\"\nbase07: \"ffffff\"\nbase08: \"ed6a43\"\nbase09: \"0086b3\"\nbase0a: \"795da3\"\nbase0b: \"183691\"\nbase0c: \"183691\"\nbase0d: \"795da3\"\nbase0e: \"a71d5d\"\nbase0f: \"333333\""
        }
        _ => return None,
    };
    ThemePalette::from_base16_str(spec).ok()
}

pub const BUILTIN_NAMES: &[&str] = &[
    "light",
    "dark",
    "catppuccin-latte",
    "catppuccin-frappe",
    "catppuccin-macchiato",
    "catppuccin-mocha",
    "solarized-light",
    "solarized-dark",
    "base16-eighties",
    "base16-ocean",
    "base16-gruvbox-dark",
    "base16-nord",
    "base16-onedark",
    "base16-dracula",
    "base16-tokyo-night",
    "base16-github",
];

/// Look up any built-in theme (including Catppuccin, Solarized, and Base16 presets) by name.
pub fn by_name(name: &str) -> Option<ThemePalette> {
    let norm = name.trim().to_ascii_lowercase().replace(['_', ' '], "-");
    match norm.as_str() {
        "light" | "default" | "auto" => Some(default_light()),
        "dark" => Some(default_dark()),
        "catppuccin-latte" => Some(curated(
            "catppuccin-latte",
            "Catppuccin Latte",
            false,
            "#eff1f5",
            "#e6e9ef",
            "#4c4f69",
            "#5c5f77",
            "#6c6f85",
            must_hex("#1e66f5"),
            must_hex("#209fb5"),
            must_hex("#40a02b"),
            must_hex("#fe640b"),
            must_hex("#df8e1d"),
            must_hex("#8839ef"),
            must_hex("#d20f39"),
            must_hex("#179299"),
            "#e6e9ef",
        )),
        "catppuccin-frappe" => Some(curated(
            "catppuccin-frappe",
            "Catppuccin Frappe",
            true,
            "#303446",
            "#292c3c",
            "#c6d0f5",
            "#b5bfe2",
            "#838ba7",
            must_hex("#8caaee"),
            must_hex("#85c1dc"),
            must_hex("#a6d189"),
            must_hex("#ef9f76"),
            must_hex("#e5c890"),
            must_hex("#ca9ee6"),
            must_hex("#e78284"),
            must_hex("#81c8be"),
            "#232634",
        )),
        "catppuccin-macchiato" => Some(curated(
            "catppuccin-macchiato",
            "Catppuccin Macchiato",
            true,
            "#24273a",
            "#1e2030",
            "#cad3f5",
            "#b8c0e0",
            "#8087a2",
            must_hex("#8aadf4"),
            must_hex("#7dc4e4"),
            must_hex("#a6da95"),
            must_hex("#f5a97f"),
            must_hex("#eed49f"),
            must_hex("#c6a0f6"),
            must_hex("#ed8796"),
            must_hex("#8bd5ca"),
            "#181926",
        )),
        "catppuccin-mocha" => Some(curated(
            "catppuccin-mocha",
            "Catppuccin Mocha",
            true,
            "#1e1e2e",
            "#181825",
            "#cdd6f4",
            "#bac2de",
            "#7f849c",
            must_hex("#89b4fa"),
            must_hex("#74c7ec"),
            must_hex("#a6e3a1"),
            must_hex("#fab387"),
            must_hex("#f9e2af"),
            must_hex("#cba6f7"),
            must_hex("#f38ba8"),
            must_hex("#94e2d5"),
            "#11111b",
        )),
        "solarized-light" => Some(curated(
            "solarized-light",
            "Solarized Light",
            false,
            "#fdf6e3",
            "#eee8d5",
            "#43575f",
            "#586e75",
            "#657b83",
            must_hex("#268bd2"),
            must_hex("#2aa198"),
            must_hex("#6a8000"),
            must_hex("#cb4b16"),
            must_hex("#b58900"),
            must_hex("#6c71c4"),
            must_hex("#dc322f"),
            must_hex("#2aa198"),
            "#eee8d5",
        )),
        "solarized-dark" => Some(curated(
            "solarized-dark",
            "Solarized Dark",
            true,
            "#002b36",
            "#073642",
            "#b6c5c5",
            "#93a1a1",
            "#657b83",
            must_hex("#268bd2"),
            must_hex("#2aa198"),
            must_hex("#859900"),
            must_hex("#cb4b16"),
            must_hex("#b58900"),
            must_hex("#6c71c4"),
            must_hex("#dc322f"),
            must_hex("#2aa198"),
            "#001f27",
        )),
        other => base16_preset(other),
    }
}

pub fn all_builtin() -> Vec<ThemePalette> {
    BUILTIN_NAMES.iter().filter_map(|n| by_name(n)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_theme_passes_wcag_shape_and_syntax_contrast() {
        for theme in all_builtin() {
            let fg = must_hex(&theme.fg);
            let code_bg = must_hex(&theme.code_bg);
            let pairs = [
                ("node", &theme.node, &theme.border),
                ("surface", &theme.surface, &theme.border),
                ("database", &theme.database, &theme.database_border),
                ("actor", &theme.actor, &theme.actor_border),
                ("cache", &theme.cache, &theme.cache_border),
                ("decision", &theme.decision, &theme.decision_border),
                ("queue", &theme.queue, &theme.border),
            ];
            for (kind, fill_hex, border_hex) in pairs {
                let fill = must_hex(fill_hex);
                let border = must_hex(border_hex);
                assert!(
                    contrast(border, fill) >= 3.0,
                    "{} {kind} border/fill contrast too low: {:.2}",
                    theme.id,
                    contrast(border, fill)
                );
                assert!(
                    contrast(fg, fill) >= 4.5,
                    "{} {kind} fg/fill contrast too low: {:.2}",
                    theme.id,
                    contrast(fg, fill)
                );
            }
            for (tok, col) in [
                ("kw", &theme.syn_kw),
                ("str", &theme.syn_str),
                ("num", &theme.syn_num),
                ("cmt", &theme.syn_cmt),
                ("type", &theme.syn_type),
                ("fn", &theme.syn_fn),
            ] {
                let c = must_hex(col);
                assert!(
                    contrast(c, code_bg) >= 4.5,
                    "{} syn-{tok}/code_bg contrast too low: {:.2}",
                    theme.id,
                    contrast(c, code_bg)
                );
            }
        }
    }

    #[test]
    fn parses_base16_yaml_and_json_formats() {
        let yaml = r##"
scheme: "Solar Flare"
variant: dark
palette:
  base00: "#18262F"
  base01: "#222E38"
  base02: "#586875"
  base03: "#667581"
  base04: "#85939E"
  base05: "#A6AFB8"
  base06: "#E8EBF0"
  base07: "#F5F7FA"
  base08: "#EF5253"
  base09: "#E66B2B"
  base0A: "#E4B51C"
  base0B: "#7CC844"
  base0C: "#52CBB0"
  base0D: "#33B5E1"
  base0E: "#A363D5"
  base0F: "#D73C9A"
"##;
        let t = ThemePalette::from_base16_str(yaml).unwrap();
        assert_eq!(t.id, "base16-solar-flare");
        assert!(t.dark);
        assert!(contrast(must_hex(&t.fg), must_hex(&t.node)) >= 4.5);

        let json = serde_json::json!({
            "scheme": "Paper",
            "base00": "F2EEDE",
            "base01": "EAE4D1",
            "base02": "D8D2BF",
            "base03": "878787",
            "base04": "5f5f5f",
            "base05": "2B2B2B",
            "base06": "1F1F1F",
            "base07": "000000",
            "base08": "CC3E28",
            "base09": "D75F00",
            "base0A": "AF8700",
            "base0B": "216609",
            "base0C": "1E6F54",
            "base0D": "1E6FCC",
            "base0E": "8700AF",
            "base0F": "5F5F00"
        })
        .to_string();
        let t_light = ThemePalette::from_base16_str(&json).unwrap();
        assert_eq!(t_light.id, "base16-paper");
        assert!(!t_light.dark);
        assert!(contrast(must_hex(&t_light.fg), must_hex(&t_light.node)) >= 4.5);
    }

    #[test]
    fn loads_base16_scheme_family_directory() {
        let dir = std::env::temp_dir().join(format!("sd-base16-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("nord.yaml"),
            "scheme: \"Nord\"\nbase00: \"2e3440\"\nbase01: \"3b4252\"\nbase02: \"434c5e\"\nbase03: \"4c566a\"\nbase04: \"d8dee9\"\nbase05: \"e5e9f0\"\nbase06: \"eceff4\"\nbase07: \"8fbcbb\"\nbase08: \"bf616a\"\nbase09: \"d08770\"\nbase0a: \"ebcb8b\"\nbase0b: \"a3be8c\"\nbase0c: \"88c0d0\"\nbase0d: \"81a1c1\"\nbase0e: \"b48ead\"\nbase0f: \"5e81ac\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("ocean.yaml"),
            "scheme: \"Ocean\"\nbase00: \"2b303b\"\nbase01: \"343d46\"\nbase02: \"4f5b66\"\nbase03: \"65737e\"\nbase04: \"a7adba\"\nbase05: \"c0c5ce\"\nbase06: \"dfe1e8\"\nbase07: \"eff1f5\"\nbase08: \"bf616a\"\nbase09: \"d08770\"\nbase0a: \"ebcb8b\"\nbase0b: \"a3be8c\"\nbase0c: \"96b5b4\"\nbase0d: \"8fa1b3\"\nbase0e: \"b48ead\"\nbase0f: \"ab7967\"\n",
        )
        .unwrap();
        let family = ThemePalette::from_base16_dir(&dir).unwrap();
        assert_eq!(family.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
