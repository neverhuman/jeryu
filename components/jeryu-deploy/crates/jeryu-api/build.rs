use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let dist = manifest_dir.join("../../apps/web/dist");
    println!("cargo:rerun-if-changed={}", dist.display());

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let out = out_dir.join("embedded_web.rs");
    let mut assets = Vec::new();

    if dist.is_dir() {
        collect_assets(&dist, &dist, &mut assets);
    }

    assets.sort_by(|left, right| left.0.cmp(&right.0));
    let mut generated = String::from("pub(crate) static ASSETS: &[EmbeddedAsset] = &[\n");
    for (route_path, file_path) in assets {
        println!("cargo:rerun-if-changed={}", file_path.display());
        generated.push_str("    EmbeddedAsset {\n");
        generated.push_str(&format!("        path: {:?},\n", route_path));
        generated.push_str(&format!(
            "        content_type: {:?},\n",
            content_type(&route_path)
        ));
        generated.push_str(&format!(
            "        bytes: include_bytes!({:?}),\n",
            file_path.display().to_string()
        ));
        generated.push_str("    },\n");
    }
    generated.push_str("];\n");

    fs::write(out, generated).expect("write embedded web asset table");
}

fn collect_assets(root: &Path, dir: &Path, assets: &mut Vec<(String, PathBuf)>) {
    for entry in fs::read_dir(dir).expect("read web dist directory") {
        let entry = entry.expect("read web dist entry");
        let path = entry.path();
        if path.is_dir() {
            collect_assets(root, &path, assets);
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .expect("asset path under web dist")
            .to_string_lossy()
            .replace('\\', "/");
        assets.push((relative, path));
    }
}

fn content_type(path: &str) -> &'static str {
    match Path::new(path).extension().and_then(|part| part.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") | Some("map") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
