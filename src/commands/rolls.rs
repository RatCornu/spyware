//! Commands used to roll dices and shows the statistics coming from them

use alloc::sync::Arc;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::str::FromStr;
use std::sync::Mutex;

use anyhow::Result;
use chrono::Utc;
use csv::{Reader, Writer};
use derive_more::{Deref, DerefMut};
use log::error;
use once_cell::sync::OnceCell;
use rand::{thread_rng, Rng};
use serenity::framework::standard::macros::command;
use serenity::framework::standard::{Args, CommandResult, Delimiter};
use serenity::model::prelude::{Message, UserId};
use serenity::model::Timestamp;
use serenity::prelude::Context;

/// Emojis needed to write "NICE" as reactions
const NICE: [char; 4] = ['🇳', '🇮', '🇨', '🇪'];

/// Name of the file containing the current session
pub static CURRENT_ROLL_SESSION: Mutex<String> = Mutex::new(String::new());

/// Common writer for the current session
pub static CURRENT_ROLL_SESSION_WRITER: OnceCell<Arc<Mutex<Writer<File>>>> = OnceCell::new();

#[derive(Debug, Clone, Copy, serde::Deserialize)]
/// Representation of a dice, used for the integration with `serde`
struct Roll {
    /// User ID of the person that rolled the dice
    user_id: u64,

    /// Result of the dice
    result: u32,

    /// Number of sides of the dice
    sides: u32,

    /// Timestamp of the roll
    #[allow(unused)]
    timestamp: Timestamp,
}

impl Roll {
    /// Returns the normalized value of the roll
    fn normalization(&self) -> f64 {
        f64::from(self.result) / f64::from(self.sides)
    }
}

#[derive(Debug, Clone, Deref, DerefMut)]
/// Wrapper around `Vec<Roll>` to implement it
struct Rolls(Vec<Roll>);

impl Rolls {
    /// Returns the normalized average of the rolls
    fn normalized_avg(&self) -> f64 {
        self.0.iter().fold(0_f64, |acc, roll| acc + roll.normalization()) / self.0.len() as f64
    }

    /// Returns the normalized standard deviation of the rolls
    fn normalized_sd(&self) -> f64 {
        self.normalized_avg()
            .mul_add(
                -self.normalized_avg(),
                (self
                    .iter()
                    .fold(0_f64, |acc, roll| roll.normalization().mul_add(roll.normalization(), acc)))
                    / self.0.len() as f64,
            )
            .sqrt()
    }

    /// Returns the avergare of the rolls
    fn avg(&self) -> f64 {
        self.0.iter().fold(0_f64, |acc, roll| acc + f64::from(roll.result)) / self.0.len() as f64
    }

    /// Returns the standard deviation of the rolls
    fn sd(&self) -> f64 {
        self.avg()
            .mul_add(
                -self.avg(),
                (self
                    .iter()
                    .fold(0_f64, |acc, roll| f64::from(roll.result).mul_add(f64::from(roll.result), acc)))
                    / self.0.len() as f64,
            )
            .sqrt()
    }
}

/// Updates the content of [`CURRENT_ROLL_SESSION`] and [`CURRENT_ROLL_SESSION_WRITER`] to match path given
fn update_session(new_file: &str) -> Result<()> {
    CURRENT_ROLL_SESSION
        .lock()
        .as_mut()
        .expect("Could not lock `CURRENT_ROLL_SESSION`")
        .clone_from(&new_file.to_owned());

    let current_roll_session_writer = CURRENT_ROLL_SESSION_WRITER.get_or_init(|| {
        Arc::new(Mutex::new(Writer::from_writer(
            File::options()
                .append(true)
                .create(true)
                .open("./rolls/".to_owned() + new_file)
                .expect("Could not open session file"),
        )))
    });

    let mut binder = current_roll_session_writer.lock().expect("Could not lock `CURRENT_ROLL_SESSION_WRITER`");
    binder.flush()?;
    *binder = Writer::from_writer(File::options().append(true).create(true).open("./rolls/".to_owned() + new_file)?);
    drop(binder);

    Ok(())
}

/// Create a new session file, appends its name in the `./rolls/sessions.txt` file, and updates [`CURRENT_ROLL_SESSION_WRITER`]
fn new_session() -> Result<()> {
    let mut session_file = File::options().read(true).append(true).create(true).open("./rolls/sessions.txt")?;
    let new_file = Utc::now().format("%Y-%m-%d_%H-%M-%S.csv").to_string();
    session_file.write_all((new_file.clone() + "\n").as_bytes())?;
    update_session(&new_file)?;

    // SAFETY: at this point, `CURRENT_ROLL_SESSION_WRITER` is initialized
    let current_roll_session_writer = unsafe { CURRENT_ROLL_SESSION_WRITER.get().unwrap_unchecked() };
    let mut binder = current_roll_session_writer.lock().expect("Could not lock `CURRENT_ROLL_SESSION_WRITER`");
    binder.write_record(["user_id", "result", "sides", "timestamp"])?;
    binder.flush()?;
    drop(binder);

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
pub fn init_csv() -> Result<()> {
    let mut session_file = File::options().read(true).append(true).create(true).open("./rolls/sessions.txt")?;
    let mut content = String::new();
    session_file.read_to_string(&mut content)?;

    match content.lines().last() {
        None => new_session()?,
        Some(session) => update_session(session)?,
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
        // SAFETY: it is checked that `results` contains at least one element
        let first_result = unsafe { iter_results.next().unwrap_unchecked() };
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
                let mut current_roll_session_writer =
                    // SAFETY: at this point, `CURRENT_ROLL_SESSION_WRITER` is initialized
                    unsafe { CURRENT_ROLL_SESSION_WRITER.get().unwrap_unchecked() }.lock().expect("Could not lock `CURRENT_ROLL_SESSION_WRITER`");
                for result in &results {
                    current_roll_session_writer
                        .write_record([
                            msg.author.id.0.to_string(),
                            result.to_string(),
                            nb_faces.to_string(),
                            msg.timestamp.to_string(),
                        ])
                        .expect("Could not write a record in the current session");
                }
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
    new_session()?;
    msg.channel_id.say(&ctx.http, "Une nouvelle session vient de débuter !").await?;

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
    // SAFETY: at this point, `CURRENT_ROLL_SESSION_WRITER` is set
    unsafe { CURRENT_ROLL_SESSION_WRITER.get_unchecked() }
        .lock()
        .as_mut()
        .expect("Could not lock `CURRENT_ROLL_SESSION_WRITER`")
        .flush()?;

    let mut is_all_rolls = false;
    let mut faces_per_dice_opt: Option<u32> = None;
    let mut user_id_opt: Option<UserId> = None;

    for packed_arg in args.iter::<String>() {
        let arg = packed_arg?;
        if arg.is_empty() {
            error!("L'argument donné est vide");
        } else if arg == "*" {
            is_all_rolls = true;
        } else if
        // SAFETY: it is checked that `arg` contains at least one char
        unsafe { arg.chars().next().unwrap_unchecked() } == 'd' {
            let mut chars = arg.chars();
            chars.next();
            faces_per_dice_opt = Some(u32::from_str(chars.as_str())?);
        } else if
        // SAFETY: the format of a user ping in discord is "\<@<u64>\>"
        unsafe { arg.chars().next().unwrap_unchecked() } == '<'
        // SAFETY: the format of a user ping in discord is "\<@<u64>\>"
            && unsafe { arg.chars().nth(1).unwrap_unchecked() } == '@'
        // SAFETY: the format of a user ping in discord is "\<@<u64>\>"
            && unsafe { arg.chars().last().unwrap_unchecked() } == '>'
        {
            let mut chars = arg.chars();
            chars.next();
            chars.next();
            chars.next_back();
            user_id_opt = Some(UserId(u64::from_str(chars.as_str())?));
        } else {
            error!("Argument \"{}\" invalide", arg);
        }
    }

    let sessions = if is_all_rolls {
        let content = fs::read_to_string("rolls/sessions.txt")?;
        let mut sessions = content.split('\n').map(ToOwned::to_owned).collect::<Vec<String>>();
        sessions.pop();
        sessions
    } else {
        let binding = CURRENT_ROLL_SESSION.lock().expect("Could not lock `CURRENT_ROLL_SESSION`");
        vec![<String as AsRef<str>>::as_ref(&binding).to_owned()]
    };

    let rolls = Rolls(
        sessions
            .into_iter()
            .flat_map(|session| load_session(&session).expect(&format!("Could not load session {}", session)))
            .filter(|roll| match (faces_per_dice_opt, user_id_opt) {
                (None, None) => true,
                (None, Some(user_id)) => roll.user_id == user_id.0,
                (Some(faces_per_dice), None) => roll.sides == faces_per_dice,
                (Some(faces_per_dice), Some(user_id)) => roll.sides == faces_per_dice && roll.user_id == user_id.0,
            })
            .collect::<Vec<Roll>>(),
    );

    match (faces_per_dice_opt, user_id_opt) {
        (None, None) => {
            msg.channel_id
                .say(
                    &ctx.http,
                    format!(
                        "Sur un total de {} dés lancés, après normalisation, la moyenne est de {} et l'écart-type de {}.",
                        rolls.len(),
                        f64::trunc(rolls.normalized_avg() * 1000_f64) / 1000_f64,
                        f64::trunc(rolls.normalized_sd() * 1000_f64) / 1000_f64
                    ),
                )
                .await?
        },
        (Some(faces_per_dice), None) => {
            msg.channel_id
                .say(
                    &ctx.http,
                    format!(
                        "Sur un total de {} de d{} lancés, la moyenne est de {} et l'écart-type de {}.",
                        rolls.len(),
                        faces_per_dice,
                        f64::trunc(rolls.avg() * 1000_f64) / 1000_f64,
                        f64::trunc(rolls.sd() * 1000_f64) / 1000_f64
                    ),
                )
                .await?
        },
        (None, Some(user_id)) => {
            msg.channel_id
                .say(
                    &ctx.http,
                    format!(
                        "Sur un total de {} dés lancés par <@{}>, après normalisation la moyenne est de {} et l'écart-type de {}.",
                        rolls.len(),
                        user_id,
                        f64::trunc(rolls.normalized_avg() * 1000_f64) / 1000_f64,
                        f64::trunc(rolls.normalized_sd() * 1000_f64) / 1000_f64
                    ),
                )
                .await?
        },
        (Some(faces_per_dice), Some(user_id)) => {
            msg.channel_id
                .say(
                    &ctx.http,
                    format!(
                        "Sur un total de {} de d{} lancés par <@{}>, la moyenne est de {} et l'écart-type de {}.",
                        rolls.len(),
                        faces_per_dice,
                        user_id,
                        f64::trunc(rolls.avg() * 1000_f64) / 1000_f64,
                        f64::trunc(rolls.sd() * 1000_f64) / 1000_f64
                    ),
                )
                .await?
        },
    };

    Ok(())
}
