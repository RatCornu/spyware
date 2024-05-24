//! Functions to interract with Suwayomi-server.

use std::collections::hash_map::Entry;
use std::env;

use anyhow::{anyhow, Result};
use graphql_client::{GraphQLQuery, Response};
use log::info;
use once_cell::sync::Lazy;
use poise::serenity_prelude::{Context, Message, Reaction, ReactionType};
use regex::Regex;
use reqwest::Client;

use crate::Data;

static SUWAYOMI_SERVER_URL: Lazy<String> = Lazy::new(|| {
    let mut base_url = env::var("SUWAYOMI_SERVER_URL").expect("SUWAYOMI_SERVER_URL is not defined");
    base_url.push_str("/api/graphql");
    base_url
});

const SPYWARE_CATEGORY_ID: i64 = 2;

const MANGA_EMOJI: &str = "📖";

const MANGADEX_REGEX: &str =
    r"https:\/\/mangadex\.org\/title\/([0-9a-fA-F]{8}\b-[0-9a-fA-F]{4}\b-[0-9a-fA-F]{4}\b-[0-9a-fA-F]{4}\b-[0-9a-fA-F]{12})\/(.*)";

const MANGADEX_SOURCE_ID: &str = "2499283573021220255";

type LongString = String;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "./graphql/suwayomi_schema.graphql",
    query_path = "./graphql/suwayomi_queries.graphql",
    response_derives = "Debug"
)]
struct SearchManga;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "./graphql/suwayomi_schema.graphql",
    query_path = "./graphql/suwayomi_queries.graphql",
    response_derives = "Debug"
)]
struct AddMangaToLibrary;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "./graphql/suwayomi_schema.graphql",
    query_path = "./graphql/suwayomi_queries.graphql",
    response_derives = "Debug"
)]
struct SetMangaCategory;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "./graphql/suwayomi_schema.graphql",
    query_path = "./graphql/suwayomi_queries.graphql",
    response_derives = "Debug"
)]
struct FetchMangaChapters;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "./graphql/suwayomi_schema.graphql",
    query_path = "./graphql/suwayomi_queries.graphql",
    response_derives = "Debug"
)]
struct AddChaptersToDownloadQueue;

/// Enumeration of all supported manga sources
#[derive(Debug)]
enum SourceType {
    /// <https://mangadex.org>
    ///
    /// Contains the name of the manga.
    Mangadex(String),
}

/// Structure containing
#[derive(Debug)]
pub struct MangaSource {
    source: SourceType,
    id: Option<i64>,
}

impl MangaSource {
    fn parse(text: &str) -> Option<Self> {
        let regex = Regex::new(MANGADEX_REGEX).unwrap();
        if let Some(capture) = regex.captures(text) {
            let name = capture.get(2)?;
            return Some(Self {
                source: SourceType::Mangadex(name.as_str().to_owned()),
                id: None,
            });
        }

        None
    }

    async fn add_to_library(&mut self) -> Result<()> {
        let client = Client::new();

        let search_request_body = match &self.source {
            SourceType::Mangadex(name) => SearchManga::build_query(search_manga::Variables {
                manga_name: Some(name.to_owned()),
                source_id: MANGADEX_SOURCE_ID.to_owned(),
            }),
        };
        let search_res = client.post(&*SUWAYOMI_SERVER_URL).json(&search_request_body).send().await?;
        let search_response_body: Response<search_manga::ResponseData> = search_res.json().await?;
        let search_response_data: search_manga::ResponseData =
            search_response_body.data.ok_or(anyhow!("Could not search this manga"))?;

        let manga_id = search_response_data.fetch_source_manga.mangas[0].id;
        self.id = Some(manga_id);

        if !search_response_data.fetch_source_manga.mangas[0].in_library {
            let pull_request_body = AddMangaToLibrary::build_query(add_manga_to_library::Variables { manga_id });
            let pull_res = client.post(&*SUWAYOMI_SERVER_URL).json(&pull_request_body).send().await?;
            let pull_response_body: Response<add_manga_to_library::ResponseData> = pull_res.json().await?;
            let pull_response_data: add_manga_to_library::ResponseData =
                pull_response_body.data.ok_or(anyhow!("Could not add this manga to the library"))?;
            info!("Manga {:?} added to the library", pull_response_data.update_manga.manga.title);

            let set_category_request_body = SetMangaCategory::build_query(set_manga_category::Variables {
                manga_id,
                category_id: SPYWARE_CATEGORY_ID,
            });
            let set_category_res = client.post(&*SUWAYOMI_SERVER_URL).json(&set_category_request_body).send().await?;
            let set_category_response_body: Response<set_manga_category::ResponseData> = set_category_res.json().await?;
            let set_category_response_data: set_manga_category::ResponseData =
                set_category_response_body.data.ok_or(anyhow!("Could not set this manga category"))?;
            info!(
                "Add manga {:?} to the \"Spyware category\"",
                set_category_response_data.update_manga_categories.manga.categories.nodes[0].name
            );
        }

        Ok(())
    }

    async fn download_all(&self) -> Result<()> {
        let manga_id = self.id.ok_or(anyhow!("Tried to download a manga before setting its id"))?;

        let client = Client::new();

        let fetch_chapters_request_body = FetchMangaChapters::build_query(fetch_manga_chapters::Variables { manga_id });
        let fetch_chapters_res = client.post(&*SUWAYOMI_SERVER_URL).json(&fetch_chapters_request_body).send().await?;
        let fetch_chapters_response_body: Response<fetch_manga_chapters::ResponseData> = fetch_chapters_res.json().await?;
        let fetch_chapters_response_data: fetch_manga_chapters::ResponseData =
            fetch_chapters_response_body.data.ok_or(anyhow!("Could not fetch this manga chapters"))?;

        let chapters = fetch_chapters_response_data
            .fetch_chapters
            .chapters
            .iter()
            .map(|chp| chp.id)
            .collect::<Vec<_>>();

        let dl_chapters_request_body =
            AddChaptersToDownloadQueue::build_query(add_chapters_to_download_queue::Variables { chapters });
        let dl_chapters_res = client.post(&*SUWAYOMI_SERVER_URL).json(&dl_chapters_request_body).send().await?;
        let dl_chapters_response_body: Response<add_chapters_to_download_queue::ResponseData> = dl_chapters_res.json().await?;
        let dl_chapters_response_data: add_chapters_to_download_queue::ResponseData = dl_chapters_response_body
            .data
            .ok_or(anyhow!("Could not add this manga to download queue"))?;

        info!(
            "Add manga {:?} to download queue, which is currently {:?}",
            manga_id, dl_chapters_response_data.enqueue_chapter_downloads.download_status.state
        );

        Ok(())
    }
}

pub async fn detection(ctx: &Context, message: &Message, data: &Data) -> Result<()> {
    if let Some(supported_source) = MangaSource::parse(&message.content) {
        let mut borrow = data.messages_with_mangas_url.lock().await;
        borrow.insert(message.id, supported_source);
        message.react(ctx, ReactionType::Unicode(MANGA_EMOJI.parse().unwrap())).await?;
    }

    Ok(())
}

pub async fn handle_reaction(ctx: &Context, reaction: &Reaction, data: &Data) -> Result<()> {
    if reaction.emoji != ReactionType::Unicode(MANGA_EMOJI.parse().unwrap()) {
        return Ok(());
    }

    let mut borrow = data.messages_with_mangas_url.lock().await;
    match borrow.entry(reaction.message_id) {
        Entry::Occupied(entry) => {
            let (_, mut manga) = entry.remove_entry();
            manga.add_to_library().await?;
            manga.download_all().await?;
            match reaction
                .message(&ctx.http)
                .await
                .unwrap()
                .reply(&ctx.http, "Ce manga a été ajouté à la liste des téléchargements")
                .await
            {
                Ok(_) => Ok(()),
                Err(err) => Err(anyhow!("{err}")),
            }
        },
        Entry::Vacant(_) => Ok(()),
    }
}
