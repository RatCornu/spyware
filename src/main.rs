//! `Spyware`

#![feature(let_chains)]

extern crate alloc;

mod commands;
mod mangas;

use std::collections::{HashMap, HashSet};
use std::env;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use log::{error, warn, Level};
use mangas::MangaSource;
use once_cell::sync::Lazy;
use serenity::client::bridge::gateway::ShardManager;
use serenity::framework::standard::macros::{group, help, hook};
use serenity::framework::standard::{help_commands, Args, CommandError, CommandGroup, CommandResult, HelpOptions};
use serenity::framework::StandardFramework;
use serenity::http::{CacheHttp, Http};
use serenity::model::channel::Reaction;
use serenity::model::id::MessageId;
use serenity::model::prelude::{Message, Ready, ResumedEvent, UserId};
use serenity::prelude::{Context, EventHandler, GatewayIntents, TypeMapKey};
use serenity::{async_trait, Client};
use songbird::SerenityInit;
use tokio::sync::Mutex;

#[allow(clippy::wildcard_imports)]
use crate::commands::misc::*;
#[allow(clippy::wildcard_imports)]
use crate::commands::music::*;
#[allow(clippy::wildcard_imports)]
use crate::commands::rolls::*;

#[group]
#[commands(ping, uptime, quoi, roll, session, stats, play, pause, resume, skip, stop, ensure)]
struct Everyone;

/// Simple event handler for serenity
#[derive(Debug)]
pub struct InnerHandler {
    messages_with_mangas_url: HashMap<MessageId, MangaSource>,
}

#[derive(Debug)]
pub struct Handler(pub Mutex<InnerHandler>);

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        warn!("{} rejoint la partie à {} !", ready.user.name, STARTING_TIME.format("%H:%M:%S"));
    }

    async fn resume(&self, _: Context, _: ResumedEvent) {
        warn!("Prêt de nouveau !");
    }

    async fn message(&self, ctx: Context, message: Message) {
        if message.author.id != ctx.http().get_current_user().await.unwrap().id {
            if let Err(err) = mangas::detection(self, &ctx, &message).await {
                error!("{err:?}");
            }
        }
    }

    async fn reaction_add(&self, ctx: Context, reaction: Reaction) {
        if reaction.user_id.unwrap() != ctx.http().get_current_user().await.unwrap().id {
            if let Err(err) = mangas::handle_reaction(self, &ctx, reaction).await {
                error!("{err:?}");
            }
        }
    }
}

pub struct ShardManagerContainer;

impl TypeMapKey for ShardManagerContainer {
    type Value = Arc<Mutex<ShardManager>>;
}

#[help]
async fn help(
    ctx: &Context,
    msg: &Message,
    args: Args,
    help_options: &'static HelpOptions,
    groups: &[&'static CommandGroup],
    owners: HashSet<UserId>,
) -> CommandResult {
    help_commands::with_embeds(ctx, msg, args, help_options, groups, owners).await?;

    Ok(())
}

#[hook]
async fn after_hook(ctx: &Context, msg: &Message, command_name: &str, result: Result<(), CommandError>) {
    if let Err(err) = result {
        msg.reply(&ctx.http, format!("Erreur : la commande a renvoyé le message suivant : `{err}`"))
            .await
            .map(|_| ())
            .unwrap_or_default();
        error!("Erreur renvoyée par la commande {} dans le message \"{}\": {}", command_name, msg.content, err);
    }
}

static DATA_DIR: Lazy<String> = Lazy::new(|| std::env::args().nth(1).expect("Must provide the data directory as argument"));

#[tokio::main]
async fn main() -> Result<()> {
    simple_logger::init_with_level(Level::Warn)?;

    let token = env::var("DISCORD_TOKEN")?;

    let http = Http::new(&token);
    let (owner, _bot_id) = match http.get_current_application_info().await {
        Ok(info) => (info.owner.id, info.id),
        Err(err) => panic!("Could not access application info: {:?}", err),
    };

    let framework = StandardFramework::new()
        .configure(|configuration| configuration.owners(HashSet::from_iter([owner])).prefix("/").allow_dm(true))
        .group(&EVERYONE_GROUP)
        .help(&HELP)
        .after(after_hook);

    let intents = GatewayIntents::all();

    init_csv()?;

    let mut client = Client::builder(token, intents)
        .framework(framework)
        .register_songbird()
        .event_handler(Handler(Mutex::new(InnerHandler {
            messages_with_mangas_url: HashMap::new(),
        })))
        .await?;

    {
        let mut data = client.data.write().await;
        data.insert::<ShardManagerContainer>(Arc::clone(&client.shard_manager));
    };

    let shard_manager = Arc::clone(&client.shard_manager);

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.expect("Could not handle SIGINT signal");
        shard_manager.lock().await.shutdown_all().await;
    });

    tokio::spawn(async {
        loop {
            tokio::time::sleep(Duration::from_secs(10 * 60)).await;
            {
                let mut current_roll_session_writer =
                    // SAFETY: at this point, `CURRENT_ROLL_SESSION_WRITER` is set
                    unsafe { CURRENT_ROLL_SESSION_WRITER.get().unwrap_unchecked() }.lock().expect("Could not lock `CURRENT_ROLL_SESSION_WRITER`");
                current_roll_session_writer
                    .flush()
                    .expect("Could not flush the current roll session writer");
            }
        }
    });

    tokio::spawn(async {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            {
                let mut current_play_modes = CURRENT_PLAY_MODES.lock().await;
                let mut call_to_leave = vec![];
                for (guild_id, ctx) in current_play_modes.iter() {
                    let manager = songbird::get(ctx).await.expect("");
                    let binding = manager.get(guild_id.to_owned()).expect("");
                    let call = binding.lock().await;
                    if call.queue().is_empty() {
                        call_to_leave.push((*guild_id, ctx.clone()));
                    }
                }

                for (guild_id, ctx) in call_to_leave {
                    let manager = songbird::get(&ctx).await.unwrap();
                    leave(guild_id, manager).await.unwrap();
                    current_play_modes.remove(&guild_id);
                }
            }
        }
    });

    if let Err(err) = client.start().await {
        error!("Client error: {:?}", err);
    }

    Ok(())
}
