fn main() {
    println!("cargo:rerun-if-changed=native/header.h");
}
