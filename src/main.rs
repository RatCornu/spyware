//! `Spyware`

#![deny(
    clippy::complexity,
    clippy::correctness,
    clippy::nursery,
    clippy::pedantic,
    clippy::perf,
    clippy::restriction,
    clippy::style
)]
#![allow(
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::blanket_clippy_restriction_lints,
    clippy::cast_precision_loss,
    clippy::else_if_without_else,
    clippy::exhaustive_enums,
    clippy::exhaustive_structs,
    clippy::expect_used,
    clippy::float_arithmetic,
    clippy::implicit_return,
    clippy::integer_division,
    clippy::match_same_arms,
    clippy::match_wildcard_for_single_variants,
    clippy::missing_trait_methods,
    clippy::mod_module_files,
    clippy::non_ascii_literal,
    clippy::panic,
    clippy::panic_in_result_fn,
    clippy::pattern_type_mismatch,
    clippy::question_mark_used,
    clippy::separated_literal_suffix,
    clippy::single_call_fn,
    clippy::shadow_reuse,
    clippy::shadow_unrelated,
    clippy::std_instead_of_core,
    clippy::string_add,
    clippy::unreachable,
    clippy::unwrap_in_result,
    clippy::wildcard_in_or_patterns,
    const_item_mutation
)]
#![cfg_attr(
    test,
    allow(
        clippy::assertions_on_result_states,
        clippy::collection_is_never_read,
        clippy::enum_glob_use,
        clippy::indexing_slicing,
        clippy::non_ascii_literal,
        clippy::too_many_lines,
        clippy::unwrap_used,
        clippy::wildcard_imports
    )
)]

extern crate alloc;

mod commands;

use alloc::sync::Arc;
use std::collections::HashSet;
use std::env;
use std::time::Duration;

use anyhow::Result;
use log::{error, warn, Level};
use serenity::client::bridge::gateway::ShardManager;
use serenity::framework::standard::macros::{group, help, hook};
use serenity::framework::standard::{help_commands, Args, CommandError, CommandGroup, CommandResult, HelpOptions};
use serenity::framework::StandardFramework;
use serenity::http::Http;
use serenity::model::prelude::{Message, Ready, ResumedEvent, UserId};
use serenity::prelude::{Context, EventHandler, GatewayIntents, TypeMapKey};
use serenity::{async_trait, Client};
use tokio::sync::Mutex;

#[allow(clippy::wildcard_imports)]
use crate::commands::misc::*;
#[allow(clippy::wildcard_imports)]
use crate::commands::rolls::*;

#[group]
#[commands(ping, uptime, roll, session, stats)]
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
        .configure(|configuration| configuration.owners(HashSet::from_iter([owner])).prefix("/").allow_dm(true))
        .group(&EVERYONE_GROUP)
        .help(&HELP)
        .after(after_hook);

    let intents = GatewayIntents::all();

    init_csv()?;

    let mut client = Client::builder(token, intents).framework(framework).event_handler(Handler).await?;

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
            tokio::time::sleep(Duration::from_secs(10)).await;
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

    if let Err(err) = client.start().await {
        error!("Client error: {:?}", err);
    }

    Ok(())
}
