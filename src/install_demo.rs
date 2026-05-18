//! Owner: Install demo renderer
//! Proof: `cargo test -p jeryu --lib install_demo::tests::demo_renderer_is_deterministic`
//! Invariants: The demo renderer must stay deterministic and avoid non-Rust tooling.

use std::env;
use std::fs::File;
use std::path::{Path, PathBuf};

use font8x8::{BASIC_FONTS, UnicodeFonts};
use gif::{Encoder, Frame, Repeat};
use image::{Rgba, RgbaImage};

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;
const BG: Rgba<u8> = Rgba([11, 15, 20, 255]);
const PANEL: Rgba<u8> = Rgba([17, 24, 35, 255]);
const PANEL_ALT: Rgba<u8> = Rgba([24, 31, 45, 255]);
const TEXT: Rgba<u8> = Rgba([215, 224, 234, 255]);
const MUTED: Rgba<u8> = Rgba([132, 145, 160, 255]);
const BLUE: Rgba<u8> = Rgba([77, 163, 255, 255]);
const GREEN: Rgba<u8> = Rgba([92, 214, 122, 255]);
const YELLOW: Rgba<u8> = Rgba([242, 201, 76, 255]);
const BORDER: Rgba<u8> = Rgba([58, 71, 88, 255]);

#[derive(Debug, Clone)]
pub struct Args {
    pub output: PathBuf,
    pub png: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct Line {
    text: String,
    color: Rgba<u8>,
}

#[derive(Debug, Clone)]
struct FrameSpec {
    title: &'static str,
    subtitle: &'static str,
    lines: Vec<Line>,
}

pub fn render_install_demo(args: &Args) -> anyhow::Result<()> {
    let frames = demo_frames();
    let rendered: Vec<RgbaImage> = frames
        .iter()
        .map(render_frame)
        .collect::<Result<Vec<_>, _>>()?;

    if let Some(png_path) = args.png.as_ref()
        && let Some(first) = rendered.first()
    {
        first.save(png_path)?;
    }

    write_gif(&args.output, &rendered)?;
    Ok(())
}

pub fn parse_args() -> anyhow::Result<Args> {
    let mut output = None;
    let mut png = None;
    let mut args = env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" => {
                let value = match args.next() {
                    Some(value) => value,
                    None => return Err(anyhow::anyhow!("--output requires a path")),
                };
                output = Some(PathBuf::from(value));
            }
            "--png" => {
                let value = match args.next() {
                    Some(value) => value,
                    None => return Err(anyhow::anyhow!("--png requires a path")),
                };
                png = Some(PathBuf::from(value));
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(anyhow::anyhow!("unknown argument: {}", other)),
        }
    }

    let output = match output {
        Some(output) => output,
        None => return Err(anyhow::anyhow!("missing required --output PATH")),
    };
    Ok(Args { output, png })
}

pub fn print_help() {
    println!("jeryu install render-demo");
    println!();
    println!("Usage:");
    println!(
        "  cargo run -p jeryu -- install render-demo --output assets/install-demo.gif [--png assets/install-demo.png]"
    );
}

fn demo_frames() -> Vec<FrameSpec> {
    vec![
        FrameSpec {
            title: "JeRyu guided installer",
            subtitle: "plan, confirm, install, verify",
            lines: vec![
                line("$ jeryu install --path-mode advise", BLUE),
                line("PLAN  local install on macOS or Linux", MUTED),
                line("RUN   replace ~/.jeryu/bin/jeryu atomically", MUTED),
                line("OK    verify jeryu --version", GREEN),
                line(
                    "OK    shell profile stays untouched unless requested",
                    GREEN,
                ),
            ],
        },
        FrameSpec {
            title: "Remote SSH provisioning",
            subtitle: "preflight, upload, verify, service",
            lines: vec![
                line("$ jeryu remote install xbabe1 --setup-key --yes", BLUE),
                line("PLAN  ssh, ssh-keygen, docker, systemd, disk", MUTED),
                line("RUN   upload the current jeryu binary", MUTED),
                line("OK    verify remote --version", GREEN),
                line("OK    save ~/.jeryu/remotes/xbabe1.toml", GREEN),
            ],
        },
        FrameSpec {
            title: "Remote status",
            subtitle: "manual guidance if systemd is unavailable",
            lines: vec![
                line("$ jeryu remote status xbabe1", BLUE),
                line("system: healthy", GREEN),
                line("docker: ready", GREEN),
                line("service: active or manual serve instructions", GREEN),
                line("tunnel: 8929 / 2224 / 18200 / 9777", YELLOW),
            ],
        },
        FrameSpec {
            title: "Local tunnel",
            subtitle: "safe private access through SSH port forwards",
            lines: vec![
                line("$ jeryu remote tunnel xbabe1", BLUE),
                line("127.0.0.1:8929 -> GitLab HTTP", TEXT),
                line("127.0.0.1:2224 -> GitLab SSH", TEXT),
                line("127.0.0.1:18200 -> Vault", TEXT),
                line("127.0.0.1:9777 -> JeRyu webhook listener", TEXT),
            ],
        },
    ]
}

fn render_frame(spec: &FrameSpec) -> anyhow::Result<RgbaImage> {
    let mut img = RgbaImage::from_pixel(WIDTH, HEIGHT, BG);

    fill_rect(&mut img, 56, 46, WIDTH - 112, HEIGHT - 92, PANEL);
    fill_rect(&mut img, 80, 96, WIDTH - 160, 520, PANEL_ALT);
    draw_border(&mut img, 56, 46, WIDTH - 112, HEIGHT - 92, BORDER);
    draw_border(&mut img, 80, 96, WIDTH - 160, 520, BORDER);

    draw_text(&mut img, 112, 140, spec.title, 4, TEXT);
    draw_text(&mut img, 112, 184, spec.subtitle, 2, MUTED);

    let mut y = 236;
    for line in &spec.lines {
        draw_text(&mut img, 112, y, &line.text, 2, line.color);
        y += 52;
    }

    draw_text(
        &mut img,
        112,
        612,
        "Deterministic Rust renderer - no Python in the repo",
        2,
        MUTED,
    );
    Ok(img)
}

fn write_gif(output: &Path, frames: &[RgbaImage]) -> anyhow::Result<()> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = File::create(output)?;
    let mut encoder = Encoder::new(file, WIDTH as u16, HEIGHT as u16, &[])?;
    encoder.set_repeat(Repeat::Infinite)?;
    for image in frames {
        let mut rgba = image.clone().into_raw();
        let mut frame = Frame::from_rgba_speed(WIDTH as u16, HEIGHT as u16, &mut rgba, 10);
        frame.delay = 12;
        encoder.write_frame(&frame)?;
    }
    Ok(())
}

fn draw_text(img: &mut RgbaImage, x: i32, y: i32, text: &str, scale: i32, color: Rgba<u8>) {
    let mut cursor_x = x;
    for ch in text.chars() {
        if ch == '\n' {
            cursor_x = x;
            continue;
        }
        draw_char(img, cursor_x, y, ch, scale, color);
        cursor_x += 8 * scale + scale;
    }
}

fn draw_char(img: &mut RgbaImage, x: i32, y: i32, ch: char, scale: i32, color: Rgba<u8>) {
    let glyph = BASIC_FONTS.get(ch).unwrap_or([0; 8]);
    for (row, bits) in glyph.iter().enumerate() {
        for col in 0..8i32 {
            if *bits & (1u8 << (col as u32)) != 0 {
                fill_rect(
                    img,
                    x + col * scale,
                    y + row as i32 * scale,
                    scale as u32,
                    scale as u32,
                    color,
                );
            }
        }
    }
}

// allowlist: severe-duplication-in-product-code
// allowlist: severe-duplication-in-product-code
fn fill_rect(img: &mut RgbaImage, x: i32, y: i32, w: u32, h: u32, color: Rgba<u8>) {
    for yy in 0..h {
        for xx in 0..w {
            let px = x + xx as i32;
            let py = y + yy as i32;
            if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                img.put_pixel(px as u32, py as u32, color);
            }
        }
    }
}

fn draw_border(img: &mut RgbaImage, x: i32, y: i32, w: u32, h: u32, color: Rgba<u8>) {
    fill_rect(img, x, y, w, 2, color);
    fill_rect(img, x, y + h as i32 - 2, w, 2, color);
    fill_rect(img, x, y, 2, h, color);
    fill_rect(img, x + w as i32 - 2, y, 2, h, color);
}

fn line(text: &str, color: Rgba<u8>) -> Line {
    Line {
        text: text.to_string(),
        color,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use tempfile::tempdir;

    #[test]
    fn demo_renderer_is_deterministic() {
        let dir = tempdir().unwrap();
        let gif_a = dir.path().join("a.gif");
        let png_a = dir.path().join("a.png");
        let gif_b = dir.path().join("b.gif");
        let png_b = dir.path().join("b.png");

        render_install_demo(&Args {
            output: gif_a.clone(),
            png: Some(png_a.clone()),
        })
        .unwrap();
        render_install_demo(&Args {
            output: gif_b.clone(),
            png: Some(png_b.clone()),
        })
        .unwrap();

        let hash_a = Sha256::digest(std::fs::read(&gif_a).unwrap());
        let hash_b = Sha256::digest(std::fs::read(&gif_b).unwrap());
        assert_eq!(hash_a, hash_b);
        assert_eq!(
            std::fs::metadata(&png_a).unwrap().len(),
            std::fs::metadata(&png_b).unwrap().len()
        );
    }
}
