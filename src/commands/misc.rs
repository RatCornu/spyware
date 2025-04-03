//! Miscellianous commands

use std::fs::File;
use std::io::{Cursor, copy};
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use poise::command;

use crate::{Context, DATA_DIR};

/// Time at which the bot is started
pub static STARTING_TIME: Lazy<DateTime<Utc>> = Lazy::new(Utc::now);

/// Directory storing all the downloaded files
static DL_DIR: Lazy<PathBuf> = Lazy::new(|| {
    let mut data_dir = PathBuf::new();
    data_dir.push(&*DATA_DIR);
    data_dir.push("downloads");
    data_dir
});

/// Displays your or another user's account creation date
#[command(prefix_command, category = "Misc")]
pub async fn ping(ctx: Context<'_>) -> Result<()> {
    ctx.say("Pong").await?;
    Ok(())
}

/// Returns the time for which the bot has been running
#[command(prefix_command, category = "Misc")]
pub async fn uptime(ctx: Context<'_>) -> Result<()> {
    let uptime_seconds = Utc::now().timestamp() - STARTING_TIME.timestamp();
    ctx.say(format!(
        "Je suis actif sans interruption depuis {}s (en gros {} jours)",
        uptime_seconds,
        uptime_seconds / 86400
    ))
    .await?;
    Ok(())
}

/// Downloads the file at the given URL
///
/// Examples:
/// * `/dl https://example.org/foo.pdf`
#[command(prefix_command, aliases("dl"), category = "Misc")]
pub async fn download(ctx: Context<'_>, url: String) -> Result<()> {
    let response = reqwest::get(url.clone()).await?;
    let Some(file_name) = url.split('/').next_back() else { return Err(anyhow!("L'")) };
    let mut file = File::create(Path::join(&DL_DIR, file_name))?;
    let mut content = Cursor::new(response.bytes().await?);
    copy(&mut content, &mut file)?;
    ctx.reply(format!("Ce fichier a été sauvegardé sous le nom `{file_name}`.")).await?;
    Ok(())
}
