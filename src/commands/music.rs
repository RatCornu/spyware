//! Commands to play music

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use serenity::framework::standard::macros::command;
use serenity::framework::standard::{Args, CommandResult};
use serenity::model::prelude::Message;
use serenity::prelude::Context;
use songbird::id::{ChannelId, GuildId};
use youtube_dl::{YoutubeDl, YoutubeDlOutput};

/// Joins the given audio channel in the given guild
async fn join<G: Into<GuildId>, C: Into<ChannelId>>(ctx: &Context, guild_id: G, channel_id: C) {
    /* // SAFETY: this command can only be executed in guilds
    let guild = unsafe { msg.guild(&ctx.cache).unwrap_unchecked() };
    let channel_id_opt = guild.voice_states.get(&msg.author.id).and_then(|voice_state| voice_state.channel_id);

    let Some(channel_id) = channel_id_opt else {
        msg.channel_id
            .say(&ctx.http, "Vous n'êtes actuellement pas dans un salon audio !")
            .await?;

        return Ok(());
    }; */

    let manager = Arc::clone(&songbird::get(ctx).await.expect("Could not get Songbird"));
    let _handler = manager.join(guild_id, channel_id).await;
}

/// Leaves the audio channel in the given guild
async fn leave<G: Into<GuildId>>(ctx: &Context, guild_id: G) -> CommandResult {
    let manager = Arc::clone(&songbird::get(ctx).await.expect("Could not get Songbird"));
    let _handler = manager.leave(guild_id).await;

    Ok(())
}

/// Downloads the video located at the given URL with youtube-dl and extracts its audio
async fn download_audio(url: String, output_folder: PathBuf) -> Result<PathBuf> {
    let mut output_file_template = output_folder.clone();
    output_file_template.push("%(id)s.%(ext)s");
    let output = YoutubeDl::new(url)
        .extract_audio(true)
        .socket_timeout("15")
        .extra_arg("--audio-format")
        .extra_arg("mp3")
        .output_template(
            output_file_template
                .as_os_str()
                .to_str()
                .expect("Could not convert the output file to a string"),
        )
        .run()?;

    match output {
        YoutubeDlOutput::SingleVideo(video) => {
            let output_file = output_folder.to_str().expect("Could not convert the output file to a string").to_owned();
            Ok(PathBuf::from(output_file + "/" + video.id.as_str() + ".mp3"))
        },
        YoutubeDlOutput::Playlist(_) => todo!(),
    }
}

#[command]
#[only_in(guilds)]
#[num_args(1)]
#[description("Joue une musique dans le channel audio de la personne ayant lancé la commande")]
#[usage("<URL complète de youtube>")]
#[example("https://www.youtube.com/watch?v=U2jF1KZNxME")]
pub async fn play(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let url = args.single::<String>()?;

    let guild = unsafe { msg.guild(&ctx.cache).unwrap_unchecked() };
    let channel_id_opt = guild.voice_states.get(&msg.author.id).and_then(|voice_state| voice_state.channel_id);

    let Some(channel_id) = channel_id_opt else {
        msg.channel_id
            .say(&ctx.http, "Vous n'êtes actuellement pas dans un salon audio !")
            .await?;

        return Ok(());
    };

    join(ctx, guild.id, channel_id).await;

    let manager = Arc::clone(&songbird::get(&ctx).await.expect("Could not get Songbird"));
    let handler_lock = manager.get(guild.id).unwrap_or_else(|| unreachable!("The guild is well defined"));
    let mut handler = handler_lock.lock().await;

    // let path = download_audio(url, "./music_cache".into()).await?;
    let input = songbird::input::ytdl(url).await?;

    let _song = handler.play_source(input);

    leave(ctx, guild.id).await?;

    Ok(())
}
