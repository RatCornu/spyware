//! Commands to play music

use alloc::sync::Arc;
use std::collections::HashMap;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use serenity::framework::standard::macros::command;
use serenity::framework::standard::{Args, CommandResult};
use serenity::model::prelude::Message;
use serenity::prelude::{Context, Mutex};
use songbird::id::{ChannelId, GuildId};
use songbird::{Call, Songbird};
use url::Url;
use youtube_dl::YoutubeDl;

use crate::DATA_DIR;

/// File extension of all music files downloaded.
pub const MUSIC_FILE_EXTENSION: &str = "mp3";

/// Timestamp at which the last song played ended.
pub static CURRENT_PLAY_MODES: Lazy<Mutex<HashMap<GuildId, Context>>> = Lazy::new(|| Mutex::new(HashMap::new()));

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

/// Leaves the audio channel in the guild defined in the given `handler`.
pub async fn leave<G: Into<GuildId> + Send>(guild_id: G, manager: Arc<Songbird>) -> Result<()> {
    manager.remove(guild_id).await?;
    Ok(())
}

/// Downloads the video located at the given URL with youtube-dl and extracts its audio.
///
/// Returns the path of the created file, the name of ID of the file and if the id is an alias.
fn find_audio_file(id: String, output_folder: &PathBuf) -> Result<(PathBuf, String, bool)> {
    let alias_path = output_folder.join(id.clone() + "." + MUSIC_FILE_EXTENSION);
    if alias_path.exists() {
        return Ok((alias_path, id, true));
    }

    let mut binding = YoutubeDl::new(&id);
    let video = binding
        .extract_audio(true)
        .socket_timeout("15")
        .extra_arg("--audio-format")
        .extra_arg(MUSIC_FILE_EXTENSION)
        .output_template("%(id)s.%(ext)s");

    let parsed_url = Url::parse(&id)?;
    let mut query = parsed_url.query_pairs();
    let Some((_, audio_id)) = query.find(|(k, _)| k == "v") else {
        return Err(anyhow!("Wrong url given : {}", id));
    };

    let output_file_path = output_folder.join(Into::<String>::into(audio_id.clone()) + "." + MUSIC_FILE_EXTENSION);
    if !output_file_path.exists() {
        video.download_to(output_folder)?;
    };

    Ok((output_file_path, audio_id.into(), false))
}

#[command]
#[only_in(guilds)]
#[num_args(1)]
#[description(
    "Joue une musique dans le channel audio de la personne ayant lancé la commande ou l'ajoute à la liste de lecture si une est déjà en cours de lecture"
)]
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

    let (path, audio_id, is_alias) = find_audio_file(url, &MUSIC_CACHE_DIR)?;
    let mut input = songbird::input::ffmpeg(path).await?;
    input.metadata.source_url = Some(if is_alias { audio_id } else { "https://youtube.com/watch?v=".to_owned() + &audio_id });

    let track_handle = handler.lock().await.enqueue_source(input);
    if let Some(web_url) = &track_handle.metadata().source_url {
        msg.channel_id
            .say(&ctx.http, format!("La musique {} a été ajouté à la file de lecture.", web_url))
            .await?;
    };

    while !track_handle.get_info().await?.playing.is_done() {}

    let mut current_play_modes = CURRENT_PLAY_MODES.lock().await;
    current_play_modes.insert(guild.id.into(), ctx.clone());
    drop(current_play_modes);

    Ok(())
}

#[command]
#[only_in(guilds)]
#[num_args(0)]
#[description("Met en pause la musique actuellement jouée")]
pub async fn pause(ctx: &Context, msg: &Message) -> CommandResult {
    // SAFETY: this command can only be run in guilds
    let guild = unsafe { msg.guild(&ctx.cache).unwrap_unchecked() };

    let manager = songbird::get(ctx).await.expect("Could not get songbird");
    let Some(handler) = manager.get(guild.id) else {
        msg.channel_id.say(&ctx.http, "Le bot n'est actuellement dans aucun salon vocal.").await?;
        return Ok(());
    };

    let binding = handler.lock().await;
    let queue = binding.queue();
    queue.pause()?;

    if queue.current().is_some() {
        msg.channel_id.say(&ctx.http, "La musique a été mis en pause.").await?;
    } else {
        msg.channel_id
            .say(&ctx.http, "Il n'y a actuellement aucune musique dans la liste de lecture.")
            .await?;
    };

    Ok(())
}

#[command]
#[only_in(guilds)]
#[num_args(0)]
#[description("Met en pause la musique actuellement jouée")]
pub async fn resume(ctx: &Context, msg: &Message) -> CommandResult {
    // SAFETY: this command can only be run in guilds
    let guild = unsafe { msg.guild(&ctx.cache).unwrap_unchecked() };

    let manager = songbird::get(ctx).await.expect("Could not get songbird");
    let Some(handler) = manager.get(guild.id) else {
        msg.channel_id.say(&ctx.http, "Le bot n'est actuellement dans aucun salon vocal.").await?;
        return Ok(());
    };

    let binding = handler.lock().await;
    let queue = binding.queue();
    queue.resume()?;

    if let Some(track) = queue.current() {
        msg.channel_id
            .say(
                &ctx.http,
                format!("La musique {} recommence à jouer.", track.metadata().source_url.as_ref().unwrap_or(&"".to_owned())),
            )
            .await?;
    } else {
        msg.channel_id
            .say(&ctx.http, "Il n'y a actuellement aucune musique dans la liste de lecture.")
            .await?;
    };

    Ok(())
}

#[command]
#[only_in(guilds)]
#[num_args(0)]
#[description("Passe la musique suivante de la liste de lecture")]
pub async fn skip(ctx: &Context, msg: &Message) -> CommandResult {
    // SAFETY: this command can only be run in guilds
    let guild = unsafe { msg.guild(&ctx.cache).unwrap_unchecked() };

    let manager = songbird::get(ctx).await.expect("Could not get songbird");
    let Some(handler) = manager.get(guild.id) else {
        msg.channel_id.say(&ctx.http, "Le bot n'est actuellement dans aucun salon vocal.").await?;
        return Ok(());
    };

    let binding = handler.lock().await;
    let queue = binding.queue();
    queue.skip()?;

    if let Some(track) = queue.current() {
        msg.channel_id
            .say(
                &ctx.http,
                format!("La musique {} commence à jouer.", track.metadata().source_url.as_ref().unwrap_or(&"".to_owned())),
            )
            .await?;
    } else {
        msg.channel_id
            .say(&ctx.http, "Il n'y a plus aucune musique dans la liste de lecture.")
            .await?;
    };

    Ok(())
}

#[command]
#[only_in(guilds)]
#[num_args(0)]
#[description("Stoppe la musique et vide la liste de lecture")]
pub async fn stop(ctx: &Context, msg: &Message) -> CommandResult {
    // SAFETY: this command can only be run in guilds
    let guild = unsafe { msg.guild(&ctx.cache).unwrap_unchecked() };

    let manager = songbird::get(ctx).await.expect("Could not get songbird");
    let Some(handler) = manager.get(guild.id) else {
        msg.channel_id.say(&ctx.http, "Le bot n'est actuellement dans aucun salon vocal.").await?;
        return Ok(());
    };

    let binding = handler.lock().await;
    let queue = binding.queue();
    queue.stop();

    msg.channel_id.say(&ctx.http, "La liste de lecture a été vidée.").await?;

    Ok(())
}

#[command]
#[only_in(guilds)]
#[min_args(1)]
#[max_args(2)]
#[description(
    "S'assure qu'une musique est téléchargée pour pouvoir la jouer instantanément, et lui donne optionnellement un alias."
)]
#[usage("<URL complète de youtube> <alias>")]
#[example("https://www.youtube.com/watch?v=U2jF1KZNxME osu")]
pub async fn ensure(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let url = args.single::<String>()?;

    if args.remaining() > 0 {
        let alias = args.single::<String>()?;
        let (file_path, audio_id, _) = find_audio_file(url, &MUSIC_CACHE_DIR)?;
        let mut symlink_path = file_path.clone().parent().unwrap_or_else(|| unreachable!()).to_path_buf();
        symlink_path.push(format!("{alias}.{MUSIC_FILE_EXTENSION}"));
        symlink(file_path, symlink_path)?;
        msg.channel_id
            .say(
                &ctx.http,
                format!(
                    "La musique https://youtube.com/watch?v={} est bien disponible et l'alias `{}` a été ajouté.",
                    audio_id, alias
                ),
            )
            .await?;
    } else {
        let (_, audio_id, _) = find_audio_file(url, &MUSIC_CACHE_DIR)?;
        msg.channel_id
            .say(&ctx.http, format!("La musique https://youtube.com/watch?v={} est bien disponible.", audio_id))
            .await?;
    }

    Ok(())
}
