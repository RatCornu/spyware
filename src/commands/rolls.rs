use std::fs::File;
use std::io::{Read, Write};
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use chrono::Utc;
use csv::{Reader, Writer};
use log::error;
use once_cell::sync::OnceCell;
use rand::{thread_rng, Rng};
use serenity::framework::standard::macros::command;
use serenity::framework::standard::{Args, CommandResult, Delimiter};
use serenity::model::prelude::{Message, UserId};
use serenity::model::Timestamp;
use serenity::prelude::Context;

const NICE: [char; 4] = ['🇳', '🇮', '🇨', '🇪'];

/// Name of the file containing the current session
pub static CURRENT_ROLL_SESSION: Mutex<String> = Mutex::new(String::new());

/// Common writer for the current session
pub static CURRENT_ROLL_SESSION_WRITER: OnceCell<Arc<Mutex<Writer<File>>>> = OnceCell::new();

#[derive(Debug, serde::Deserialize)]
struct Roll {
    user_id: u64,
    result: u32,
    sides: u32,
    timestamp: Timestamp,
}

/// Updates the content of [`CURRENT_ROLL_SESSION`] and [`CURRENT_ROLL_SESSION_WRITER`] to match path given
async fn update_session(new_file: &str) -> Result<()> {
    CURRENT_ROLL_SESSION.lock().as_mut().unwrap().clone_from(&new_file.to_owned());

    let current_roll_session_writer = CURRENT_ROLL_SESSION_WRITER.get_or_init(|| {
        Arc::new(Mutex::new(Writer::from_writer(
            File::options().append(true).create(true).open("./rolls/".to_owned() + &new_file).unwrap(),
        )))
    });

    let mut binder = current_roll_session_writer.lock().unwrap();
    binder.flush()?;
    *binder = Writer::from_writer(File::options().append(true).create(true).open("./rolls/".to_owned() + &new_file).unwrap());

    Ok(())
}

/// Create a new session file, appends its name in the `./rolls/sessions.txt` file, and updates [`CURRENT_ROLL_SESSION_WRITER`]
async fn new_session() -> Result<()> {
    let mut session_file = File::options().read(true).append(true).create(true).open("./rolls/sessions.txt")?;
    let new_file = Utc::now().format("%Y-%m-%d_%H-%M-%S.csv").to_string();
    session_file.write_all((new_file.clone() + "\n").as_bytes())?;
    update_session(&new_file).await?;

    Ok(())
}

/// Take the name of a session file and return the content as a roll vector
fn load_session(file: &str) -> Result<Vec<Roll>> {
    let mut rolls = Vec::<Roll>::new();
    let session_file = File::options().read(true).open("./rolls/".to_owned() + file)?;
    let mut reader = Reader::from_reader(session_file);
    for result in reader.deserialize::<Roll>() {
        rolls.push(result?);
    }
    Ok(rolls)
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
    let nb_dices = parsed_args.single::<u32>().unwrap();
    let nb_faces = parsed_args.single::<u32>().unwrap();

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
                    sent_message.react(&ctx.http, emoji).await.unwrap();
                }
            },
            Ok(_) => {
                let mut current_roll_session_writer = CURRENT_ROLL_SESSION_WRITER.get().unwrap().lock().unwrap();
                results.iter().for_each(|result| {
                    current_roll_session_writer
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
                msg.channel_id.say(&ctx.http, "Le nombre de dés jetés est trop grand !").await.unwrap();
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
    new_session().await.unwrap();
    msg.channel_id.say(&ctx.http, "Une nouvelle session vient de débuter !").await.unwrap();

    Ok(())
}

#[command]
#[min_args(0)]
#[max_args(3)]
#[description("Affiche des statistiques relatives aux jets de dés")]
#[usage("(*) (@user) (d<nombre de faces>)")]
#[example("* @test d100")]
#[example("d50 @test")]
#[example("* d100")]
pub async fn stats(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let mut is_all_rolls = false;
    let mut faces_per_dice: Option<u32> = None;
    let mut user_id: Option<UserId> = None;

    for packed_arg in args.iter::<String>() {
        let arg = packed_arg.unwrap();
        if arg.is_empty() {
            error!("L'argument donné est vide");
        } else if arg == "*" {
            is_all_rolls = true;
        } else if arg.chars().nth(0).unwrap() == 'd' {
            let mut chars = arg.chars();
            chars.next();
            faces_per_dice = Some(u32::from_str(chars.as_str()).unwrap());
        } else if arg.chars().nth(0).unwrap() == '<' && arg.chars().nth(1).unwrap() == '@' && arg.chars().last().unwrap() == '>' {
            let mut chars = arg.chars();
            chars.next();
            chars.next();
            chars.next_back();
            user_id = Some(UserId(u64::from_str(chars.as_str()).unwrap()))
        } else {
            error!("Argument \"{}\" invalide", arg);
        }
    }

    let sessions = if is_all_rolls {
        let mut sessions = File::options().read(true).open("rolls/sessions.txt").unwrap();
        let mut content = String::new();
        sessions.read_to_string(&mut content).unwrap();
        let mut sessions = content.split('\n').map(ToOwned::to_owned).collect::<Vec<String>>();
        sessions.pop();
        sessions
    } else {
        let binding = CURRENT_ROLL_SESSION.lock().expect("Could not lock `CURRENT_ROLL_SESSION`");
        vec![<String as AsRef<str>>::as_ref(&binding).to_owned().clone()]
    };

    todo!("Renvoyer les données");

    Ok(())
}
