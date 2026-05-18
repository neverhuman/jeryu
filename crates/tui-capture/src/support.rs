use image::{Rgba, RgbaImage};
use rusttype::{Font, PositionedGlyph, Scale, point};
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

pub(crate) fn fill_rect(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, color: Rgb8) {
    let x2 = x.saturating_add(w).min(img.width());
    let y2 = y.saturating_add(h).min(img.height());
    for yy in y..y2 {
        for xx in x..x2 {
            img.put_pixel(xx, yy, rgba(color));
        }
    }
}

pub(crate) fn rgba(c: Rgb8) -> Rgba<u8> {
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

pub(crate) fn boost_rgb(c: Rgb8, factor: f32, add: f32) -> Rgb8 {
    let f = |v: u8| (v as f32 * factor + add).round().clamp(0.0, 255.0) as u8;
    Rgb8 {
        r: f(c.r),
        g: f(c.g),
        b: f(c.b),
    }
}

pub(crate) fn scale_rgb(c: Rgb8, factor: f32) -> Rgb8 {
    let f = |v: u8| (v as f32 * factor).round().clamp(0.0, 255.0) as u8;
    Rgb8 {
        r: f(c.r),
        g: f(c.g),
        b: f(c.b),
    }
}

// ---------------------------------------------------------------------------
// Color table, utility functions, and tests (extracted to companion)
// ---------------------------------------------------------------------------

#[path = "support_utils.rs"]
mod support_utils;
pub(crate) use support_utils::*;
