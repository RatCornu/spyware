//! Miscellianous commands

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serenity::framework::standard::macros::command;
use serenity::framework::standard::CommandResult;
use serenity::model::prelude::Message;
use serenity::prelude::Context;

/// Time at which the bot is started
pub static STARTING_TIME: Lazy<DateTime<Utc>> = Lazy::new(Utc::now);

#[command]
#[num_args(0)]
#[description("Teste que le bot est bien connecté")]
pub async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    msg.channel_id.say(&ctx.http, "Pong").await?;

    Ok(())
}

#[command]
#[num_args(0)]
#[description("Renvoie le temps depuis lequel le bot est en ligne")]
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

#[command]
#[num_args(0)]
#[description("Commande nulle à chier")]
pub async fn quoi(ctx: &Context, msg: &Message) -> CommandResult {
    msg.reply(&ctx.http, "Quoicoubeh").await?;

    Ok(())
}
