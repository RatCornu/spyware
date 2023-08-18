mod commands;

use std::collections::HashSet;
use std::env;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use commands::misc::*;
use commands::rolls::*;
use log::{error, warn, Level};
use serenity::client::bridge::gateway::ShardManager;
use serenity::framework::standard::macros::{group, help};
use serenity::framework::standard::{help_commands, Args, CommandGroup, CommandResult, HelpOptions};
use serenity::framework::StandardFramework;
use serenity::http::Http;
use serenity::model::prelude::{Message, Ready, ResumedEvent, UserId};
use serenity::prelude::{Context, EventHandler, GatewayIntents, TypeMapKey};
use serenity::{async_trait, Client};
use tokio::sync::Mutex;

#[group]
#[commands(ping, uptime, roll, session, stats)]
struct Everyone;

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        warn!("{} rejoint la partie à {} !", ready.user.name, STARTING_TIME.format("%H:%M:%S"));
    }

    async fn resume(&self, _: Context, _: ResumedEvent) {
        warn!("Prêt de nouveau !")
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

#[tokio::main]
async fn main() -> Result<()> {
    simple_logger::init_with_level(Level::Warn)?;
    dotenv::dotenv()?;

    let token = env::var("DISCORD_TOKEN")?;

    let http = Http::new(&token);
    let (owner, _bot_id) = match http.get_current_application_info().await {
        Ok(info) => (info.owner.id, info.id),
        Err(err) => panic!("Could not access application info: {:?}", err),
    };

    let framework = StandardFramework::new()
        .configure(|c| c.owners(HashSet::from_iter([owner])).prefix("/").allow_dm(true))
        .group(&EVERYONE_GROUP)
        .help(&HELP);

    let intents = GatewayIntents::all();

    init_csv().await?;

    let mut client = Client::builder(token, intents).framework(framework).event_handler(Handler).await?;

    {
        let mut data = client.data.write().await;
        data.insert::<ShardManagerContainer>(client.shard_manager.clone());
    }

    let shard_manager = client.shard_manager.clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        shard_manager.lock().await.shutdown_all().await;
    });

    tokio::spawn(async {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            {
                let mut current_roll_session_writer = CURRENT_ROLL_SESSION_WRITER.get().unwrap().lock().unwrap();
                current_roll_session_writer.flush().unwrap();
            }
        }
    });

    if let Err(err) = client.start().await {
        error!("Client error: {:?}", err);
    }

    Ok(())
}
