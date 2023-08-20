//! Commands to play music

use std::sync::Arc;

use serenity::framework::standard::macros::command;
use serenity::framework::standard::CommandResult;
use serenity::model::prelude::Message;
use serenity::prelude::Context;

#[command]
#[only_in(guilds)]
async fn join(ctx: &Context, msg: &Message) -> CommandResult {
    // SAFETY: this command can only be executed in guilds
    let guild = unsafe { msg.guild(&ctx.cache).unwrap_unchecked() };
    let channel_id_opt = guild.voice_states.get(&msg.author.id).and_then(|voice_state| voice_state.channel_id);

    let Some(channel_id) = channel_id_opt else {
        msg.channel_id
            .say(&ctx.http, "Vous n'êtes actuellement pas dans un salon audio !")
            .await?;

        return Ok(());
    };

    let manager = Arc::clone(&songbird::get(ctx).await.expect("Could not get Songbird"));
    let _handler = manager.join(guild.id, channel_id).await;

    Ok(())
}
