// build.rs — embed git/CI build metadata into the binary at compile time.
//
// When the environment variable JERYU_BUILD_META is set (e.g. to "g7abc1234"
// by the CI release pipeline), the compiled binary reports:
//
//   jeryu --version  →  "jeryu 3.3.23+g7abc1234"
//
// Without the variable (local dev builds) it falls back to the plain Cargo
// package version:
//
//   jeryu --version  →  "jeryu 3.3.23"
//
// Consumers use env!("JERYU_FULL_VERSION") instead of env!("CARGO_PKG_VERSION").

fn main() {
    let base =
        std::env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION is always set by Cargo");

    let meta = std::env::var("JERYU_BUILD_META")
        .ok()
        .filter(|m| !m.is_empty() && m != "dev");

    let full = match meta {
        Some(m) => format!("{base}+{m}"),
        None => base,
    };

    println!("cargo:rustc-env=JERYU_FULL_VERSION={full}");
    // Re-run this script whenever the meta env var changes.
    println!("cargo:rerun-if-env-changed=JERYU_BUILD_META");
}
