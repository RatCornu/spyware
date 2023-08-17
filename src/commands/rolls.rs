use std::{
    fs::File,
    io::{Read, Write},
    sync::{Arc, Mutex},
};

use anyhow::Result;
use chrono::Utc;
use csv::Writer;
use log::error;
use once_cell::sync::OnceCell;
use rand::{thread_rng, Rng};
use serenity::{
    framework::standard::{macros::command, Args, CommandResult, Delimiter},
    model::{prelude::Message, Timestamp},
    prelude::Context,
};

const NICE: [char; 4] = ['🇳', '🇮', '🇨', '🇪'];

pub static CURRENT_ROLL_SESSION: OnceCell<Arc<Mutex<Writer<File>>>> = OnceCell::new();

/// Updates the content of [`CURRENT_ROLL_SESSION`] to match path given
async fn update_session(new_file: &str) -> Result<()> {
    let current_roll_session = CURRENT_ROLL_SESSION.get_or_init(|| {
        Arc::new(Mutex::new(Writer::from_writer(
            File::options().append(true).create(true).open("./rolls/".to_owned() + &new_file).unwrap(),
        )))
    });

    let mut binder = current_roll_session.lock().unwrap();
    binder.flush()?;
    *binder = Writer::from_writer(File::options().append(true).create(true).open("./rolls/".to_owned() + &new_file).unwrap());

    Ok(())
}

/// Create a new session file, appends its name in the `./rolls/sessions.txt` file, and updates [`CURRENT_ROLL_SESSION`]
async fn new_session() -> Result<()> {
    let mut session_file = File::options().read(true).append(true).create(true).open("./rolls/sessions.txt")?;
    let new_file = Utc::now().format("%Y-%m-%d_%H-%M-%S.csv").to_string();
    session_file.write_all((new_file.clone() + "\n").as_bytes())?;
    update_session(&new_file).await?;

    Ok(())
}

/// Initializes the roll saving system in CSV files
pub async fn init_csv() -> Result<()> {
    let mut session_file = File::options().read(true).append(true).create(true).open("./rolls/sessions.txt")?;
    let mut content = String::new();
    session_file.read_to_string(&mut content)?;

    match content.lines().last() {
        None => new_session().await?,
        Some(session) => update_session(session).await?,
    };

    Ok(())
}

#[command]
#[aliases("r")]
#[num_args(1)]
#[description("Jette des dés")]
#[usage("<Nombre de dés>d<Nombre de faces par dé>")]
#[example("1d100")]
pub async fn roll(ctx: &Context, msg: &Message, rolls: Args) -> CommandResult {
    let mut parsed_args = Args::new(rolls.message(), &[Delimiter::Single('d'), Delimiter::Single(' ')]);
    let nb_dices = parsed_args.single::<u32>()?;
    let nb_faces = parsed_args.single::<u32>()?;

    let mut results = Vec::<u32>::new();
    for _ in 0..nb_dices {
        results.push(thread_rng().gen_range(1..=nb_faces));
    }

    if nb_dices == 0 {
        error!("Tried to roll 0 dice");
    } else {
        let mut iter_results = results.iter();
        let first_result = iter_results.next().unwrap();
        match msg
            .channel_id
            .say(
                &ctx.http,
                format!(
                    "{}\n> {}",
                    msg.author.name,
                    iter_results.fold(first_result.to_string(), |acc, res| acc + " / " + &res.to_string())
                ),
            )
            .await
        {
            Ok(sent_message) if nb_dices == 1 && first_result == &69 => {
                for emoji in NICE {
                    sent_message.react(&ctx.http, emoji).await?;
                }
            },
            Ok(_) => {
                let mut current_roll_session = CURRENT_ROLL_SESSION.get().unwrap().lock().unwrap();
                results.iter().for_each(|result| {
                    current_roll_session
                        .write_record([
                            msg.author.id.0.to_string(),
                            result.to_string(),
                            nb_faces.to_string(),
                            msg.timestamp.to_string(),
                        ])
                        .unwrap();
                });
            },
            Err(_) => {
                error!("Tried to roll {}d{} which is too large for one message", nb_dices, nb_faces);
                msg.channel_id.say(&ctx.http, "Le nombre de dés jetés est trop grand !").await?;
            },
        }
    }

    Ok(())
}

#[command]
#[num_args(0)]
#[description("Crée une nouvelle session de jets")]
#[usage("")]
pub async fn session(ctx: &Context, msg: &Message) -> CommandResult {
    new_session().await?;
    msg.channel_id.say(&ctx.http, "Une nouvelle session vient de débuter !").await?;

    Ok(())
}

#[derive(Debug, serde::Deserialize)]
struct Roll {
    user_id: u64,
    result: u32,
    sides: u32,
    timestamp: Timestamp,
}
