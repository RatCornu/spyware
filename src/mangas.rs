//! Functions to interract with Suwayomi-server.

use std::collections::hash_map::Entry;

use anyhow::Result;
use regex::Regex;
use serenity::client::Context;
use serenity::model::channel::{Message, Reaction, ReactionType};

use crate::Handler;

const MANGA_EMOJI: &str = "📖";

const MANGADEX_REGEX: &str =
    r"https:\/\/mangadex\.org\/title\/([0-9a-fA-F]{8}\b-[0-9a-fA-F]{4}\b-[0-9a-fA-F]{4}\b-[0-9a-fA-F]{4}\b-[0-9a-fA-F]{12})\/(.*)";

/// Enumeration of all supported manga sources
#[derive(Debug)]
pub enum MangaSource {
    /// <https://mangadex.org>
    ///
    /// Contains the UUID of the manga.
    Mangadex(String),
}

fn is_a_supported_manga_url(text: &str) -> Option<MangaSource> {
    let regex = Regex::new(MANGADEX_REGEX).unwrap();
    if let Some(capture) = regex.captures(text) {
        let uuid = capture.get(1)?;
        return Some(MangaSource::Mangadex(uuid.as_str().to_owned()));
    }

    None
}

pub async fn detection(handler: &Handler, ctx: &Context, message: &Message) -> Result<()> {
    if let Some(supported_source) = is_a_supported_manga_url(&message.content) {
        let mut handle = handler.0.lock().await;
        handle.messages_with_mangas_url.insert(message.id, supported_source);
        message.react(ctx, ReactionType::Unicode(MANGA_EMOJI.parse().unwrap())).await?;
    }

    Ok(())
}

pub async fn handle_reaction(handler: &Handler, ctx: &Context, reaction: Reaction) -> Result<()> {
    let mut handle = handler.0.lock().await;
    match handle.messages_with_mangas_url.entry(reaction.message_id) {
        Entry::Occupied(entry) => {
            let (_, manga) = entry.remove_entry();
            match manga {
                MangaSource::Mangadex(uuid) => {
                    println!("{uuid:?}");
                    Ok(())
                },
            }
        },
        Entry::Vacant(_) => Ok(()),
    }
}
