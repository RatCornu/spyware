//! Commands to play music

use alloc::sync::Arc;
use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use chrono::Utc;
use once_cell::sync::Lazy;
use serenity::framework::standard::macros::command;
use serenity::framework::standard::{Args, CommandResult};
use serenity::model::prelude::Message;
use serenity::prelude::{Context, Mutex};
use songbird::id::{ChannelId, GuildId};
use songbird::{Call, Songbird};
use youtube_dl::{YoutubeDl, YoutubeDlOutput};

use crate::DATA_DIR;

/// Timestamp at which the last song played ended
pub static CURRENT_PLAY_MODES: Lazy<Mutex<HashMap<GuildId, (Context, i64)>>> = Lazy::new(|| Mutex::new(HashMap::new()));


/// Directory used to cache musics
static MUSIC_CACHE_DIR: Lazy<PathBuf> = Lazy::new(|| {
    let mut data_dir = PathBuf::new();
    data_dir.push(&*DATA_DIR);
    data_dir.push("music_cache");
    data_dir
});

/// Joins the given audio channel in the given guild
async fn join<G: Into<GuildId> + Send, C: Into<ChannelId> + Send>(
    ctx: &Context,
    guild_id: G,
    channel_id: C,
) -> (Arc<Songbird>, Arc<Mutex<Call>>) {
    let manager = Arc::clone(&songbird::get(ctx).await.expect("Could not get Songbird"));
    let (handler, _) = manager.join(guild_id, channel_id).await;
    (manager, handler)
}

/// Joins the given audio channel in the given guild and set the bot as deaf
async fn join_deaf<G: Into<GuildId> + Send, C: Into<ChannelId> + Send>(
    ctx: &Context,
    guild_id: G,
    channel_id: C,
) -> Result<(Arc<Songbird>, Arc<Mutex<Call>>)> {
    let (manager, handler) = join(ctx, guild_id, channel_id).await;
    handler.lock().await.deafen(true).await?;
    Ok((manager, handler))
}

/// Leaves the audio channel in the guild defined in the given `handler`
pub async fn leave<G: Into<GuildId> + Send>(guild_id: G, manager: Arc<Songbird>) -> Result<()> {
    manager.remove(guild_id).await?;
    Ok(())
}

/// Downloads the video located at the given URL with youtube-dl and extracts its audio
fn download_audio(url: String, output_folder: &PathBuf) -> Result<PathBuf> {
    let mut binding = YoutubeDl::new(url);
    let video = binding
        .extract_audio(true)
        .socket_timeout("15")
        .extra_arg("--audio-format")
        .extra_arg("mp3")
        .output_template("%(id)s.%(ext)s");

    let output = video.run()?;

    video.download_to(output_folder)?;

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

    // SAFETY: this command can only be run in guilds
    let guild = unsafe { msg.guild(&ctx.cache).unwrap_unchecked() };
    let channel_id_opt = guild.voice_states.get(&msg.author.id).and_then(|voice_state| voice_state.channel_id);

    let mut current_play_modes = CURRENT_PLAY_MODES.lock().await;
    current_play_modes.remove(&guild.id.into());
    drop(current_play_modes);

    let Some(channel_id) = channel_id_opt else {
        msg.channel_id
            .say(&ctx.http, "Vous n'êtes actuellement pas dans un salon audio !")
            .await?;

        return Ok(());
    };

    let (_manager, handler) = join_deaf(ctx, guild.id, channel_id).await?;

    let path = download_audio(url, &MUSIC_CACHE_DIR)?;
    let input = songbird::input::ffmpeg(path).await?;

    let track_handle = handler.lock().await.play_source(input);

    while !track_handle.get_info().await?.playing.is_done() {}

    let mut current_play_modes = CURRENT_PLAY_MODES.lock().await;
    current_play_modes.insert(guild.id.into(), (ctx.clone(), Utc::now().timestamp()));
    drop(current_play_modes);

    Ok(())
}
