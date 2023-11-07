mod openai;
mod sermaid;

use std::path::PathBuf;

use clap::Parser;
use color_eyre::eyre::{Context, Result};
use food::bin::ConfigPathGetter;
use serde::Deserialize;
use sermaid::SerMaid;

const CARGO_PKG_NAME: &str = env!("CARGO_PKG_NAME");

#[derive(Parser)]
#[command(version, about)]
pub struct Args {
    /// Specify configuration file
    #[arg(short, long, value_name = "FILE", default_value = "./config.toml")]
    pub config: PathBuf,
}

impl ConfigPathGetter for Args {
    fn config_path(&self) -> &std::path::Path {
        &self.config
    }
}

#[derive(Deserialize)]
struct Config {
    api_token: String,
    history_file: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    food::log::init(CARGO_PKG_NAME).wrap_err_with(|| "failed to initialize food::log")?;

    let (_, config): (Args, Config) = food::bin::get_args_and_config()
        .wrap_err_with(|| "failed to initialize arguments and config")?;

    SerMaid::from_config(config)?.run().await
}
