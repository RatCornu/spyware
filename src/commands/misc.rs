use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serenity::{
    framework::standard::{macros::command, CommandResult},
    model::prelude::Message,
    prelude::Context,
};

pub static STARTING_TIME: Lazy<DateTime<Utc>> = Lazy::new(|| Utc::now());

#[command]
pub async fn test(ctx: &Context, msg: &Message) -> CommandResult {
    msg.channel_id.say(&ctx.http, "Test").await?;

    Ok(())
}

#[command]
pub async fn uptime(ctx: &Context, msg: &Message) -> CommandResult {
    let uptime_seconds = Utc::now().timestamp() - STARTING_TIME.timestamp();

    msg.channel_id
        .say(
            &ctx.http,
            format!("Je suis actif sans interruption depuis {}s (en gros {} jours)", uptime_seconds, uptime_seconds / 86400),
        )
        .await?;

    Ok(())
}
