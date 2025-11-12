//! `Spyware`

#![feature(try_blocks)]

use std::collections::{HashMap, HashSet};
use std::env;
use std::sync::Arc;

use anyhow::{Error, Result};
use commands::custom::cthulhu_dark::rcd;
use log::{Level, error, warn};
use mangas::MangaSource;
use once_cell::sync::Lazy;
use poise::serenity_prelude::futures::lock::Mutex;
use poise::serenity_prelude::{CacheHttp, ClientBuilder, FullEvent, GatewayIntents, Http, MessageId};
use poise::{Framework, FrameworkContext, FrameworkOptions, PrefixFrameworkOptions, builtins, command};
use songbird::Songbird;

use crate::commands::cards::draw;
use crate::commands::misc::{STARTING_TIME, ping, uptime};
use crate::commands::music::{ensure, pause, play, resume, skip, stop};
use crate::commands::rolls::{init_csv, private_roll, roll, roll_twice, session, stats};

mod commands;
mod mangas;

static DATA_DIR: Lazy<String> =
    Lazy::new(|| std::env::args().nth(1).expect("Must provide the data directory as argument"));

struct Data {
    songbird: Arc<Songbird>,
    messages_with_mangas_url: Mutex<HashMap<MessageId, MangaSource>>,
}

type Context<'a> = poise::Context<'a, Data, Error>;

async fn event_handler(
    ctx: &poise::serenity_prelude::Context,
    event: &FullEvent,
    _framework: FrameworkContext<'_, Data, Error>,
    data: &Data,
) -> Result<()> {
    match event {
        FullEvent::Message { new_message } => {
            if new_message.author.id != ctx.http().get_current_user().await.unwrap().id {
                mangas::detection(ctx, new_message, data).await
            } else {
                Ok(())
            }
        },
        FullEvent::ReactionAdd { add_reaction } => {
            if add_reaction.user_id.unwrap() != ctx.http().get_current_user().await.unwrap().id {
                mangas::handle_reaction(ctx, add_reaction, data).await
            } else {
                Ok(())
            }
        },
        _ => Ok(()),
    }
}

#[command(prefix_command, category = "Misc")]
async fn help(
    ctx: Context<'_>,
    #[description = "Commande pour laquelle il faut donner plus d'informations"] command: Option<String>,
) -> Result<()> {
    let config = builtins::HelpConfiguration::default();
    builtins::help(ctx, command.as_deref(), config).await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    simple_logger::init_with_level(Level::Warn)?;

    let token = env::var("DISCORD_TOKEN")?;

    let http = Http::new(&token);
    let (owner, bot_name) = match http.get_current_application_info().await {
        Ok(info) => (info.owner.unwrap().id, info.name),
        Err(err) => panic!("Could not access application info: {:?}", err),
    };

    let songbird = Songbird::serenity();
    let songbird_clone = Arc::clone(&songbird);

    let framework = Framework::builder()
        .options(FrameworkOptions {
            prefix_options: PrefixFrameworkOptions {
                prefix: Some("/".into()),
                edit_tracker: Some(Arc::new(poise::EditTracker::for_timespan(std::time::Duration::from_secs(3600)))),
                case_insensitive_commands: true,
                ..Default::default()
            },
            owners: HashSet::from_iter([owner]),
            commands: vec![
                help(),
                ping(),
                uptime(),
                // download(),
                roll(),
                private_roll(),
                roll_twice(),
                session(),
                stats(),
                draw(),
                play(),
                ensure(),
                pause(),
                resume(),
                skip(),
                stop(),
                rcd(),
            ],
            event_handler: |ctx, event, framework, data| Box::pin(event_handler(ctx, event, framework, data)),
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data {
                    songbird: songbird_clone,
                    messages_with_mangas_url: Mutex::new(HashMap::new()),
                })
            })
        })
        .build();

    let intents = GatewayIntents::MESSAGE_CONTENT | GatewayIntents::non_privileged();

    let mut client = ClientBuilder::new(token, intents)
        .framework(framework)
        .voice_manager_arc(songbird)
        .await?;

    init_csv().await?;

    warn!("{} a démaré à : {}.", bot_name, *STARTING_TIME);
    if let Err(err) = client.start().await {
        error!("Client error: {:?}", err);
    }

    Ok(())
}
