use std::path::PathBuf;

use jeryu_jira::contracts::{CONTRACT_COUNT, export_all_contracts};
use ts_rs::Config;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = match std::env::var_os("TS_RS_EXPORT_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("contracts")
            .join("generated"),
    };

    std::fs::create_dir_all(&out_dir)?;
    let cfg = Config::new().with_out_dir(&out_dir);
    export_all_contracts(&cfg)?;
    println!(
        "exported {CONTRACT_COUNT} Work contracts to {}",
        out_dir.display()
    );
    Ok(())
}
