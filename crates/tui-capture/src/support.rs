use anyhow::{Result, bail};
use image::{Rgba, RgbaImage};
use rusttype::{Font, PositionedGlyph, Scale, point};
use std::path::PathBuf;
use vt100::{Color as VtColor, Screen};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rgb8 {
    pub(crate) r: u8,
    pub(crate) g: u8,
    pub(crate) b: u8,
}

#[derive(Clone, Copy)]
pub(crate) struct RenderConfig {
    pub(crate) scale: Scale,
    pub(crate) cell_w: u32,
    pub(crate) cell_h: u32,
    pub(crate) padding: u32,
    pub(crate) default_bg: Rgb8,
    pub(crate) default_fg: Rgb8,
    pub(crate) brighten: f32,
    pub(crate) respect_dim: bool,
}

pub(crate) struct CellText<'a> {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) text: &'a str,
    pub(crate) color: Rgb8,
    pub(crate) bold: bool,
    pub(crate) underline: bool,
}

pub(crate) fn render_screen(screen: &Screen, font: &Font<'_>, cfg: &RenderConfig) -> RgbaImage {
    let (rows, cols) = screen.size();
    let width = cfg.padding * 2 + u32::from(cols) * cfg.cell_w;
    let height = cfg.padding * 2 + u32::from(rows) * cfg.cell_h;
    let mut img = RgbaImage::from_pixel(width, height, rgba(cfg.default_bg));

    for row in 0..rows {
        for col in 0..cols {
            if let Some(cell) = screen.cell(row, col) {
                let (_, bg) = cell_colors(cell, cfg);
                fill_rect(
                    &mut img,
                    cfg.padding + u32::from(col) * cfg.cell_w,
                    cfg.padding + u32::from(row) * cfg.cell_h,
                    cfg.cell_w,
                    cfg.cell_h,
                    bg,
                );
            }
        }
    }

    for row in 0..rows {
        for col in 0..cols {
            let Some(cell) = screen.cell(row, col) else {
                continue;
            };
            if cell.is_wide_continuation() || !cell.has_contents() {
                continue;
            }
            let text = cell.contents();
            if text.is_empty() || text == " " {
                continue;
            }
            let (fg, _) = cell_colors(cell, cfg);
            draw_cell_text(
                &mut img,
                font,
                cfg,
                CellText {
                    x: cfg.padding + u32::from(col) * cfg.cell_w,
                    y: cfg.padding + u32::from(row) * cfg.cell_h,
                    text,
                    color: fg,
                    bold: cell.bold(),
                    underline: cell.underline(),
                },
            );
        }
    }

    img
}

fn cell_colors(cell: &vt100::Cell, cfg: &RenderConfig) -> (Rgb8, Rgb8) {
    let mut fg = resolve_vt_color(cell.fgcolor(), false, cfg);
    let mut bg = resolve_vt_color(cell.bgcolor(), true, cfg);
    if cell.inverse() {
        std::mem::swap(&mut fg, &mut bg);
    }
    if cell.dim() && cfg.respect_dim {
        fg = scale_rgb(fg, 0.62);
    }
    if cell.bold() {
        fg = boost_rgb(fg, 1.08, 4.0);
    }
    (fg, bg)
}

fn draw_cell_text(img: &mut RgbaImage, font: &Font<'_>, cfg: &RenderConfig, cell: CellText<'_>) {
    let v_metrics = font.v_metrics(cfg.scale);
    let line_h = v_metrics.ascent - v_metrics.descent;
    let baseline = cell.y as f32 + ((cfg.cell_h as f32 - line_h) / 2.0).floor() + v_metrics.ascent;
    let mut caret = cell.x as f32;

    for ch in cell.text.chars().filter(|ch| !ch.is_control()) {
        let glyph = font.glyph(ch).scaled(cfg.scale);
        let advance = glyph.h_metrics().advance_width;
        draw_glyph(img, glyph.positioned(point(caret, baseline)), cell.color);
        if cell.bold {
            let bold_glyph = font
                .glyph(ch)
                .scaled(cfg.scale)
                .positioned(point(caret + 0.7, baseline));
            draw_glyph(img, bold_glyph, cell.color);
        }
        caret += advance;
    }

    if cell.underline {
        fill_rect(
            img,
            cell.x,
            cell.y + cfg.cell_h.saturating_sub(3),
            cfg.cell_w,
            2,
            cell.color,
        );
    }
}

fn draw_glyph(img: &mut RgbaImage, glyph: PositionedGlyph<'_>, color: Rgb8) {
    let Some(bb) = glyph.pixel_bounding_box() else {
        return;
    };
    glyph.draw(|gx, gy, coverage| {
        let px = bb.min.x + gx as i32;
        let py = bb.min.y + gy as i32;
        if px < 0 || py < 0 {
            return;
        }
        let px = px as u32;
        let py = py as u32;
        if px >= img.width() || py >= img.height() {
            return;
        }
        alpha_blend(img.get_pixel_mut(px, py), color, coverage);
    });
}

fn alpha_blend(dst: &mut Rgba<u8>, src: Rgb8, alpha: f32) {
    let alpha = alpha.clamp(0.0, 1.0);
    let inv = 1.0 - alpha;
    dst.0[0] = (src.r as f32 * alpha + dst.0[0] as f32 * inv).round() as u8;
    dst.0[1] = (src.g as f32 * alpha + dst.0[1] as f32 * inv).round() as u8;
    dst.0[2] = (src.b as f32 * alpha + dst.0[2] as f32 * inv).round() as u8;
    dst.0[3] = 255;
}

fn fill_rect(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, color: Rgb8) {
    let x2 = x.saturating_add(w).min(img.width());
    let y2 = y.saturating_add(h).min(img.height());
    for yy in y..y2 {
        for xx in x..x2 {
            img.put_pixel(xx, yy, rgba(color));
        }
    }
}

fn rgba(c: Rgb8) -> Rgba<u8> {
    Rgba([c.r, c.g, c.b, 255])
}

fn resolve_vt_color(c: VtColor, is_bg: bool, cfg: &RenderConfig) -> Rgb8 {
    let raw = match c {
        VtColor::Default => {
            if is_bg {
                cfg.default_bg
            } else {
                cfg.default_fg
            }
        }
        VtColor::Idx(i) => xterm_256_color(i, cfg.default_bg),
        VtColor::Rgb(r, g, b) => Rgb8 { r, g, b },
    };
    if is_bg {
        match c {
            VtColor::Default | VtColor::Idx(0) => cfg.default_bg,
            _ => boost_rgb(raw, 1.06, 0.0),
        }
    } else {
        boost_rgb(raw, cfg.brighten, 6.0)
    }
}

fn xterm_256_color(idx: u8, default_black: Rgb8) -> Rgb8 {
    const ANSI16: [Rgb8; 16] = [
        Rgb8 {
            r: 18,
            g: 25,
            b: 35,
        },
        Rgb8 {
            r: 255,
            g: 86,
            b: 96,
        },
        Rgb8 {
            r: 64,
            g: 230,
            b: 125,
        },
        Rgb8 {
            r: 255,
            g: 238,
            b: 88,
        },
        Rgb8 {
            r: 95,
            g: 174,
            b: 255,
        },
        Rgb8 {
            r: 255,
            g: 108,
            b: 255,
        },
        Rgb8 {
            r: 70,
            g: 235,
            b: 255,
        },
        Rgb8 {
            r: 232,
            g: 238,
            b: 245,
        },
        Rgb8 {
            r: 120,
            g: 132,
            b: 148,
        },
        Rgb8 {
            r: 255,
            g: 112,
            b: 122,
        },
        Rgb8 {
            r: 88,
            g: 255,
            b: 145,
        },
        Rgb8 {
            r: 255,
            g: 255,
            b: 112,
        },
        Rgb8 {
            r: 125,
            g: 195,
            b: 255,
        },
        Rgb8 {
            r: 255,
            g: 140,
            b: 255,
        },
        Rgb8 {
            r: 105,
            g: 250,
            b: 255,
        },
        Rgb8 {
            r: 255,
            g: 255,
            b: 255,
        },
    ];
    match idx {
        0 => default_black,
        1..=15 => ANSI16[usize::from(idx)],
        16..=231 => {
            let i = idx - 16;
            let conv = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            Rgb8 {
                r: conv(i / 36),
                g: conv((i % 36) / 6),
                b: conv(i % 6),
            }
        }
        232..=255 => {
            let v = 8 + (idx - 232) * 10;
            Rgb8 { r: v, g: v, b: v }
        }
    }
}

fn boost_rgb(c: Rgb8, factor: f32, add: f32) -> Rgb8 {
    let f = |v: u8| (v as f32 * factor + add).round().clamp(0.0, 255.0) as u8;
    Rgb8 {
        r: f(c.r),
        g: f(c.g),
        b: f(c.b),
    }
}

fn scale_rgb(c: Rgb8, factor: f32) -> Rgb8 {
    let f = |v: u8| (v as f32 * factor).round().clamp(0.0, 255.0) as u8;
    Rgb8 {
        r: f(c.r),
        g: f(c.g),
        b: f(c.b),
    }
}

pub(crate) fn parse_hex_rgb(s: &str) -> Result<Rgb8> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        bail!("expected #RRGGBB color, got {s:?}");
    }
    Ok(Rgb8 {
        r: u8::from_str_radix(&s[0..2], 16)?,
        g: u8::from_str_radix(&s[2..4], 16)?,
        b: u8::from_str_radix(&s[4..6], 16)?,
    })
}

pub(crate) fn decode_escapes(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut it = s.as_bytes().iter().copied();
    while let Some(b) = it.next() {
        if b != b'\\' {
            out.push(b);
            continue;
        }
        match it.next() {
            Some(b'r') => out.push(b'\r'),
            Some(b'n') => out.push(b'\n'),
            Some(b't') => out.push(b'\t'),
            Some(b'e') => out.push(0x1b),
            Some(b'\\') => out.push(b'\\'),
            Some(other) => {
                out.push(b'\\');
                out.push(other);
            }
            None => out.push(b'\\'),
        }
    }
    out
}

pub(crate) fn clamp_u16(v: u32) -> u16 {
    v.min(u32::from(u16::MAX)) as u16
}

pub(crate) fn find_font() -> Option<PathBuf> {
    [
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansMono-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoMono-Regular.ttf",
        "/usr/share/fonts/truetype/liberation2/LiberationMono-Regular.ttf",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|path| path.exists())
}

pub(crate) fn validate_required_glyphs(font: &Font<'_>) -> Result<()> {
    let required = [
        '─', '│', '┌', '┐', '└', '┘', '├', '┤', '┬', '┴', '┼', '█', '░', '▒', '▓', '▁', '▂', '▃',
        '▄', '▅', '▆', '▇', '●',
    ];
    let missing: String = required
        .into_iter()
        .filter(|ch| font.glyph(*ch).id().0 == 0)
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        bail!("selected font is missing required TUI glyphs: {missing:?}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn hex_color_parser_accepts_hash() {
        assert_eq!(
            parse_hex_rgb("#17212b").unwrap(),
            Rgb8 {
                r: 0x17,
                g: 0x21,
                b: 0x2b
            }
        );
    }

    #[test]
    fn escape_decoder_handles_terminal_sequences() {
        assert_eq!(
            decode_escapes(r"\e[2J\r\n\t\\"),
            b"\x1b[2J\r\n\t\\".to_vec()
        );
    }

    #[test]
    fn default_font_has_required_glyphs_when_installed() {
        let Some(path) = find_font() else {
            return;
        };
        let bytes = fs::read(path).unwrap();
        let font = Font::try_from_vec(bytes).unwrap();
        validate_required_glyphs(&font).unwrap();
    }
}
