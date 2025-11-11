//! Miscellianous commands

use anyhow::Result;
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use poise::command;

use crate::Context;

/// Time at which the bot is started
pub static STARTING_TIME: Lazy<DateTime<Utc>> = Lazy::new(Utc::now);

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
