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

#[test]
fn demo_renderer_creates_nested_output_dirs() {
    let dir = tempdir().unwrap();
    let gif = dir.path().join("nested/gif/install-demo.gif");
    let png = dir.path().join("nested/png/install-demo.png");

    render_install_demo(&Args {
        output: gif.clone(),
        png: Some(png.clone()),
    })
    .unwrap();

    assert!(gif.exists());
    assert!(png.exists());
}
