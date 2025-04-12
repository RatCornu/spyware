//! Commands to play music

use std::os::unix::fs::symlink;
use std::path::PathBuf;

use anyhow::{Result, anyhow};
use log::warn;
use once_cell::sync::Lazy;
use poise::command;
use url::Url;
use youtube_dl::YoutubeDl;

use crate::{Context, DATA_DIR};

/// File extension of all music files downloaded.
pub const MUSIC_FILE_EXTENSION: &str = "mp3";

/// Directory used to cache musics
static MUSIC_CACHE_DIR: Lazy<PathBuf> = Lazy::new(|| {
    let mut data_dir = PathBuf::new();
    data_dir.push(&*DATA_DIR);
    data_dir.push("music_cache");
    data_dir
});

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
        warn!("Downloading the audio {id} to {output_file_path:?}");
        video.download_to(output_folder)?;
    };

    Ok((output_file_path, audio_id.into(), false))
}

/// Joue une musique depuis youtube.
///
/// Joue une musique depuis une URL youtube dans le channel audio de la personne ayant lancé la commande, ou l'ajoute à
/// la liste de lectyre si une musique est déja en cours de lecture.
///
/// Si un alias a été donné à la musique, l'alias peut remplacer l'URL.
///
/// Exemples :
/// * `/play https://www.youtube.com/watch?v=U2jF1KZNxME
/// * `/play <alias>`
#[command(prefix_command, guild_only, category = "Musique")]
pub async fn play(ctx: Context<'_>, url: String) -> Result<()> {
    let guild_id = ctx.guild().unwrap().id;
    let channel_id = ctx
        .guild()
        .unwrap()
        .voice_states
        .get(&ctx.author().id)
        .ok_or(anyhow!("Rejoignez un salon vocal avant de lancer démarrer une lecture !"))?
        .channel_id
        .ok_or(anyhow!("Aucune idée de ce qu'il se passe"))?;

    let manager = &ctx.data().songbird;
    let handler = manager.join(guild_id, channel_id).await?;
    handler.lock().await.deafen(true).await?;

    let (path, audio_id, _is_alias) = find_audio_file(url, &MUSIC_CACHE_DIR)?;
    warn!("Starts playing the audio {path:?}");
    let input = songbird::input::File::new(path).into();

    let mut borrow = handler.lock().await;
    let track_handle = borrow.enqueue(input).await;
    drop(borrow);
    ctx.say(format!("La musique {}{} a été ajouté à la file de lecture.", "https://youtube.com/watch?v=", &audio_id))
        .await?;

    while !track_handle.get_info().await?.playing.is_done() {}

    Ok(())
}

/// Mets en pause la musique actuellement jouée
#[command(prefix_command, guild_only, category = "Musique")]
pub async fn pause(ctx: Context<'_>) -> Result<()> {
    // SAFETY: this command can only be run in guilds
    let guild_id = unsafe { ctx.guild().unwrap_unchecked().id };

    let manager = &ctx.data().songbird;
    let Some(handler) = manager.get(guild_id) else {
        ctx.say("Le bot n'est actuellement dans aucun salon vocal.").await?;
        return Ok(());
    };

    let binding = handler.lock().await;
    let queue = binding.queue();
    queue.pause()?;

    if queue.current().is_some() {
        ctx.say("La musique a été mis en pause.").await?;
    } else {
        ctx.say("Il n'y a actuellement aucune musique dans la liste de lecture.").await?;
    };

    Ok(())
}

/// Reprends une musique mise en pause.
#[command(prefix_command, guild_only, category = "Musique")]
pub async fn resume(ctx: Context<'_>) -> Result<()> {
    // SAFETY: this command can only be run in guilds
    let guild_id = unsafe { ctx.guild().unwrap_unchecked().id };

    let manager = &ctx.data().songbird;
    let Some(handler) = manager.get(guild_id) else {
        ctx.say("Le bot n'est actuellement dans aucun salon vocal.").await?;
        return Ok(());
    };

    let binding = handler.lock().await;
    let queue = binding.queue();
    queue.resume()?;

    if let Some(track) = queue.current() {
        let source_url = track.uuid();
        ctx.say(format!("La musique {} recommence à jouer.", source_url)).await?;
    } else {
        ctx.say("Il n'y a actuellement aucune musique dans la liste de lecture.").await?;
    };

    Ok(())
}

/// Passe à la musique suivante de la liste de lecture.
#[command(prefix_command, guild_only, category = "Musique")]
pub async fn skip(ctx: Context<'_>) -> Result<()> {
    // SAFETY: this command can only be run in guilds
    let guild_id = unsafe { ctx.guild().unwrap_unchecked().id };

    let manager = &ctx.data().songbird;
    let Some(handler) = manager.get(guild_id) else {
        ctx.say("Le bot n'est actuellement dans aucun salon vocal.").await?;
        return Ok(());
    };

    let binding = handler.lock().await;
    let queue = binding.queue();
    queue.skip()?;

    if let Some(track) = queue.current() {
        let source_url = track.uuid();
        ctx.say(format!("La musique {} commence à jouer.", source_url)).await?;
    } else {
        ctx.say("Il n'y a plus aucune musique dans la liste de lecture.").await?;
    };

    Ok(())
}

/// Stoppe la musique, vide la liste de lecture et fait quitter le bot du salon audio.
#[command(prefix_command, guild_only, category = "Musique")]
pub async fn stop(ctx: Context<'_>) -> Result<()> {
    // SAFETY: this command can only be run in guilds
    let guild_id = unsafe { ctx.guild().unwrap_unchecked().id };

    let manager = &ctx.data().songbird;
    let Some(handler) = manager.get(guild_id) else {
        ctx.say("Le bot n'est actuellement dans aucun salon vocal.").await?;
        return Ok(());
    };

    let binding = handler.lock().await;
    let queue = binding.queue();
    queue.stop();

    ctx.say("La liste de lecture a été vidée.").await?;

    manager.remove(guild_id).await?;

    Ok(())
}

/// S'assure de la disponibilité d'une musique.
///
/// S'assure qu'une musique est téléchargée pour pouvoir la jouer instantanément par la suite.
///
/// Permet également de donner un alias à la musique pour pouvoir l'écrire à la place de l'URL dans l'invocation de la
/// commande `/play`.
///
/// Exemples :
/// * `/ensure https://www.youtube.com/watch?v=U2jF1KZNxME`
/// * `/ensure https://www.youtube.com/watch?v=U2jF1KZNxME osu` puis `/play osu`
#[command(prefix_command, guild_only, category = "Musique")]
pub async fn ensure(ctx: Context<'_>, url: String, alias_opt: Option<String>) -> Result<()> {
    if let Some(alias) = alias_opt {
        let (file_path, audio_id, _) = find_audio_file(url, &MUSIC_CACHE_DIR)?;
        warn!("Ensuring the audio {file_path:?}");
        let mut symlink_path = file_path.clone().parent().unwrap_or_else(|| unreachable!()).to_path_buf();
        symlink_path.push(format!("{alias}.{MUSIC_FILE_EXTENSION}"));
        symlink(file_path, symlink_path)?;
        ctx.say(format!(
            "La musique https://youtube.com/watch?v={} est bien disponible et l'alias `{}` a été ajouté.",
            audio_id, alias
        ))
        .await?;
    } else {
        let (path, audio_id, _) = find_audio_file(url, &MUSIC_CACHE_DIR)?;
        warn!("Ensuring the audio {path:?}");
        ctx.say(format!("La musique https://youtube.com/watch?v={} est bien disponible.", audio_id))
            .await?;
    }

    Ok(())
}
