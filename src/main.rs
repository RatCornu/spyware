mod commands;

use std::{collections::HashSet, env, sync::Arc};

use anyhow::Result;
use log::{error, warn, Level};
use serenity::{
    async_trait,
    client::bridge::gateway::ShardManager,
    framework::{
        standard::{
            help_commands,
            macros::{group, help},
            Args, CommandGroup, CommandResult, HelpOptions,
        },
        StandardFramework,
    },
    http::Http,
    model::prelude::{Message, Ready, ResumedEvent, UserId},
    prelude::{Context, EventHandler, GatewayIntents, TypeMapKey},
    Client,
};
use tokio::sync::Mutex;

use commands::misc::*;
use commands::rolls::*;

#[group]
#[commands(ping, uptime, roll)]
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

    if let Err(err) = client.start().await {
        error!("Client error: {:?}", err);
    }

    Ok(())
}
