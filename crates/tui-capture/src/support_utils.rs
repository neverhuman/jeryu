use anyhow::{Result, bail};
use rusttype::Font;
use std::path::PathBuf;

use super::Rgb8;

pub(crate) fn xterm_256_color(idx: u8, default_black: Rgb8) -> Rgb8 {
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
        let bytes = std::fs::read(path).unwrap();
        let font = Font::try_from_vec(bytes).unwrap();
        validate_required_glyphs(&font).unwrap();
    }
}
