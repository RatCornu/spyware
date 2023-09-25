//! `Spyware`

extern crate alloc;

mod commands;

use alloc::sync::Arc;
use std::collections::{HashMap, HashSet};
use std::env;
use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use log::{error, warn, Level};
use once_cell::sync::Lazy;
use serenity::client::bridge::gateway::ShardManager;
use serenity::framework::standard::macros::{group, help, hook};
use serenity::framework::standard::{help_commands, Args, CommandError, CommandGroup, CommandResult, HelpOptions};
use serenity::framework::StandardFramework;
use serenity::http::Http;
use serenity::model::prelude::{Message, Ready, ResumedEvent, UserId};
use serenity::prelude::{Context, EventHandler, GatewayIntents, TypeMapKey};
use serenity::{async_trait, Client};
use songbird::id::GuildId;
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
struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        warn!("{} rejoint la partie à {} !", ready.user.name, STARTING_TIME.format("%H:%M:%S"));
    }

    async fn resume(&self, _: Context, _: ResumedEvent) {
        warn!("Prêt de nouveau !");
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
async fn after_hook(_ctx: &Context, msg: &Message, command_name: &str, result: Result<(), CommandError>) {
    if let Err(err) = result {
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
        .event_handler(Handler)
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
                let mut contextx = HashMap::<GuildId, Context>::new();
                let mut should_leave = Vec::<GuildId>::new();
                for (guild_id, (ctx, _timestamp)) in &*current_play_modes {
                    contextx.insert(*guild_id, ctx.clone());
                }
                current_play_modes.retain(|guild_id, (_ctx, timestamp)| {
                    if Utc::now().timestamp() - *timestamp > 5 * 60 {
                        should_leave.push(*guild_id);
                        false
                    } else {
                        true
                    }
                });
                drop(current_play_modes);
                for guild_id in should_leave {
                    let ctx = contextx.get(&guild_id).expect("");
                    let manager = songbird::get(ctx).await.expect("");
                    if let Err(err) = leave(guild_id, manager).await {
                        error!("Error in guild {guild_id}: {err:?}");
                    }
                }
            }
        }
    });

    if let Err(err) = client.start().await {
        error!("Client error: {:?}", err);
    }

    Ok(())
}
