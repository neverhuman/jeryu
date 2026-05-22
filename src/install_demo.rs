//! Owner: Install demo renderer
//! Proof: `cargo test -p jeryu --lib install_demo::tests::demo_renderer_is_deterministic`
//! Invariants: The demo renderer must stay deterministic and avoid non-Rust tooling.

use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Args {
    pub output: PathBuf,
    pub png: Option<PathBuf>,
}

pub fn render_install_demo(args: &Args) -> anyhow::Result<()> {
    render::render_install_demo(args)
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

#[path = "install_demo_render.rs"]
mod render;

#[cfg(test)]
#[path = "install_demo_tests.rs"]
mod tests;
