use anyhow::Result;
use neo_install::cli::run;

#[tokio::main]
pub async fn main() -> Result<()> {
    run().await
}
