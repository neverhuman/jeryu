use anyhow::Result;

use super::*;

#[path = "runtime_reports_collect.rs"]
mod collect;

impl SmartCache {
    pub async fn status(&self) -> Result<()> {
        self.status_with_options(false).await
    }

    pub async fn status_with_options(&self, json: bool) -> Result<()> {
        let report = self.status_report(None).await?;
        if json {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            print_cache_status_report(&report);
        }
        Ok(())
    }
}
