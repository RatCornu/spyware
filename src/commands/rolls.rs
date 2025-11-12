//! Commands used to roll dices and shows the statistics coming from them

use std::collections::HashMap;
use std::fmt::Display;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::ops::{Add, Div, Mul, Rem, Sub};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, LazyLock};

use anyhow::{Result, anyhow};
use async_recursion::async_recursion;
use chrono::Utc;
use csv::{Reader, Writer};
use derive_more::{Deref, DerefMut};
use log::error;
use pest::Parser;
use pest::iterators::Pairs;
use pest::pratt_parser::PrattParser;
use plotters::backend::BitMapBackend;
use plotters::chart::ChartBuilder;
use plotters::coord::ranged1d::IntoSegmentedCoord;
use plotters::drawing::IntoDrawingArea;
use plotters::series::Histogram;
use plotters::style::{BLUE, Color, IntoFont, WHITE};
use poise::command;
use poise::serenity_prelude::futures::lock::Mutex;
use poise::serenity_prelude::{CreateAttachment, CreateMessage, Timestamp, UserId};
use rand::{Rng, thread_rng};
use tempfile::NamedTempFile;
use tokio::sync::OnceCell;

use crate::{Context, DATA_DIR};

/// Emojis needed to write "NICE" as reactions
const NICE: [char; 4] = ['🇳', '🇮', '🇨', '🇪'];

/// Directory used to store roll linked files
static ROLL_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let mut data_dir = PathBuf::new();
    data_dir.push(&*DATA_DIR);
    data_dir.push("rolls");
    data_dir
});

/// Name of the file containing the current session
pub static CURRENT_ROLL_SESSION: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::new()));

/// Common writer for the current session
pub static CURRENT_ROLL_SESSION_WRITER: LazyLock<OnceCell<Arc<Mutex<Writer<File>>>>> = LazyLock::new(OnceCell::new);

/// Representation of a dice, used for the integration with `serde`
#[derive(Debug, Clone, Copy, serde::Deserialize)]
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

impl PartialEq for Roll {
    fn eq(&self, other: &Self) -> bool {
        self.user_id == other.user_id && self.result == other.result && self.sides == other.sides
    }
}

impl Eq for Roll {}

impl PartialOrd for Roll {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Roll {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.result.cmp(&other.result)
    }
}

impl Roll {
    /// Returns the normalized value of the roll
    fn normalization(&self) -> f64 {
        f64::from(self.result) / f64::from(self.sides)
    }
}

/// Wrapper around `Vec<Roll>` to implement it
#[derive(Debug, Clone, Deref, DerefMut)]
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

/// Derivation of the pest grammar.
#[derive(pest_derive::Parser)]
#[grammar = "./src/commands/rolls.pest"]
pub struct Calculator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOperator {
    Minus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    Add,
    Substract,
    Multiply,
    Divide,
    Modulo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dices {
    nb_dices: u32,
    nb_faces: u32,
}

/// Expressions available during a roll.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr<T> {
    Dices(T),
    Integer(u32),
    UnOp {
        op: UnaryOperator,
        exp: Box<Expr<T>>,
    },
    BinOp {
        lhs: Box<Expr<T>>,
        op: BinaryOperator,
        rhs: Box<Expr<T>>,
    },
}

impl Display for Expr<Vec<u32>> {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Expr::Dices(rolls) => {
                if let Some(first_roll) = rolls.first() {
                    let mut acc = first_roll.to_string();
                    for r in &rolls[1..] {
                        acc.push_str(&format!(", {r}"));
                    }
                    write!(fmt, "[{}]", acc)
                } else {
                    write!(fmt, "[]")
                }
            },
            Expr::Integer(int) => write!(fmt, "{int}"),
            Expr::UnOp { op, exp } => write!(
                fmt,
                "{} {}",
                match op {
                    UnaryOperator::Minus => "-",
                },
                exp
            ),
            Expr::BinOp { lhs, op, rhs } => write!(
                fmt,
                "{} {} {}",
                lhs,
                match op {
                    BinaryOperator::Add => "+",
                    BinaryOperator::Substract => "-",
                    BinaryOperator::Multiply => "*",
                    BinaryOperator::Divide => "/",
                    BinaryOperator::Modulo => "%",
                },
                rhs
            ),
        }
    }
}

impl Expr<Dices> {
    #[async_recursion]
    pub async fn eval(&self, ctx: &Context<'_>) -> Result<Expr<Vec<u32>>> {
        match self {
            Expr::Dices(Dices { nb_dices, nb_faces }) => {
                let mut rolls = Vec::new();
                for _ in 0..*nb_dices {
                    rolls.push(roll_dice(ctx, *nb_faces).await?);
                }
                Ok(Expr::Dices(rolls))
            },
            Expr::Integer(int) => Ok(Expr::Integer(*int)),
            Expr::UnOp { op, exp } => match op {
                UnaryOperator::Minus => Ok(Expr::UnOp {
                    op: UnaryOperator::Minus,
                    exp: Box::new(exp.eval(ctx).await?),
                }),
            },
            Expr::BinOp { lhs, op, rhs } => Ok(Expr::BinOp {
                lhs: Box::new(lhs.eval(ctx).await?),
                op: *op,
                rhs: Box::new(rhs.eval(ctx).await?),
            }),
        }
    }
}

impl Expr<Vec<u32>> {
    pub fn value(&self) -> i64 {
        match self {
            Expr::Dices(rolls) => i64::from(rolls.iter().sum::<u32>()),
            Expr::Integer(int) => i64::from(*int),
            Expr::UnOp { op, exp } => match op {
                UnaryOperator::Minus => -exp.value(),
            },
            Expr::BinOp { lhs, op, rhs } => {
                let op_fn = match op {
                    BinaryOperator::Add => <i64 as Add<i64>>::add,
                    BinaryOperator::Substract => <i64 as Sub<i64>>::sub,
                    BinaryOperator::Multiply => <i64 as Mul<i64>>::mul,
                    BinaryOperator::Divide => <i64 as Div<i64>>::div,
                    BinaryOperator::Modulo => <i64 as Rem<i64>>::rem,
                };

                op_fn(lhs.value(), rhs.value())
            },
        }
    }
}

static PRATT_PARSER: LazyLock<PrattParser<Rule>> = std::sync::LazyLock::new(|| {
    use pest::pratt_parser::Assoc::Left;
    use pest::pratt_parser::Op;

    PrattParser::new()
        .op(Op::infix(Rule::add, Left) | Op::infix(Rule::subtract, Left))
        .op(Op::infix(Rule::multiply, Left) | Op::infix(Rule::divide, Left) | Op::infix(Rule::modulo, Left))
        .op(Op::prefix(Rule::unary_minus))
});

pub fn parse_expr(pairs: Pairs<'_, Rule>) -> Result<Expr<Dices>> {
    PRATT_PARSER
        .map_primary(|primary| try {
            match primary.as_rule() {
                Rule::integer => Expr::Integer(primary.as_str().parse::<u32>()?),
                Rule::dice => {
                    let mut dice_roll = primary.as_str().split('d');
                    let nb_dices =
                        dice_roll.next().ok_or(anyhow!("Could not parse roll dice"))?.parse::<u32>().unwrap();
                    let nb_faces =
                        dice_roll.next().ok_or(anyhow!("Could not parse roll dice"))?.parse::<u32>().unwrap();

                    Expr::Dices(Dices { nb_dices, nb_faces })
                },
                Rule::expr => parse_expr(primary.into_inner())?,
                rule => return Err(anyhow!("Entrée invalide: {rule:?}")),
            }
        })
        .map_prefix(|op, rhs| try {
            match op.as_rule() {
                Rule::unary_minus => Expr::UnOp {
                    op: UnaryOperator::Minus,
                    exp: Box::new(rhs?),
                },
                rule => return Err(anyhow!("Entrée invalide: {rule:?}")),
            }
        })
        .map_infix(|lhs, op, rhs| try {
            let op = match op.as_rule() {
                Rule::add => BinaryOperator::Add,
                Rule::subtract => BinaryOperator::Substract,
                Rule::multiply => BinaryOperator::Multiply,
                Rule::divide => BinaryOperator::Divide,
                Rule::modulo => BinaryOperator::Modulo,
                rule => return Err(anyhow!("Expr::parse expected infix operation, found {rule:?}")),
            };

            Expr::BinOp {
                lhs: Box::new(lhs?),
                op,
                rhs: Box::new(rhs?),
            }
        })
        .parse(pairs)
}

/// Updates the content of [`CURRENT_ROLL_SESSION`] and [`CURRENT_ROLL_SESSION_WRITER`] to match path given
async fn update_session(new_file: &str) -> Result<()> {
    CURRENT_ROLL_SESSION.lock().await.clone_from(&new_file.to_owned());

    let new_file_path = Path::join(&ROLL_DIR, new_file);
    let current_roll_session_writer = CURRENT_ROLL_SESSION_WRITER.get_or_init(async move || {
        Arc::new(Mutex::new(Writer::from_writer(
            File::options()
                .append(true)
                .create(true)
                .open(new_file_path)
                .expect("Could not open session file"),
        )))
    });

    let new_file_path = Path::join(&ROLL_DIR, new_file);
    let mut binder = current_roll_session_writer.await.lock().await;
    binder.flush()?;
    *binder = Writer::from_writer(File::options().append(true).create(true).open(new_file_path)?);
    drop(binder);

    Ok(())
}

/// Create a new session file, appends its name in the `<DATA_DIR>/rolls/sessions.txt` file, and updates
/// [`CURRENT_ROLL_SESSION_WRITER`]
async fn new_session() -> Result<()> {
    let session_file_path = Path::join(&ROLL_DIR, "sessions.txt");
    let mut session_file = File::options().read(true).append(true).create(true).open(session_file_path)?;
    let new_file = Utc::now().format("%Y-%m-%d_%H-%M-%S.csv").to_string();
    session_file.write_all((new_file.clone() + "\n").as_bytes())?;
    update_session(&new_file).await?;

    // SAFETY: at this point, `CURRENT_ROLL_SESSION_WRITER` is initialized
    let current_roll_session_writer = unsafe { CURRENT_ROLL_SESSION_WRITER.get().unwrap_unchecked() };
    let mut binder = current_roll_session_writer.lock().await;
    binder.write_record(["user_id", "result", "sides", "timestamp"])?;
    binder.flush()?;
    drop(binder);

    Ok(())
}

/// Take the name of a session file and return the content as a roll vector
fn load_session(file: &str) -> Result<Vec<Roll>> {
    let mut rolls = Vec::<Roll>::new();
    let session_file_path = Path::join(&ROLL_DIR, file);
    let session_file = File::options().read(true).open(session_file_path)?;
    let mut reader = Reader::from_reader(session_file);
    for result in reader.deserialize::<Roll>() {
        rolls.push(result?);
    }
    Ok(rolls)
}

/// Initializes the roll saving system in CSV files
#[allow(clippy::verbose_file_reads)]
pub async fn init_csv() -> Result<()> {
    let session_file_path = Path::join(&ROLL_DIR, "sessions.txt");
    let mut session_file = File::options().read(true).append(true).create(true).open(session_file_path)?;
    let mut content = String::new();
    session_file.read_to_string(&mut content)?;

    match content.lines().last() {
        None => new_session().await?,
        Some(session) => update_session(session).await?,
    };

    Ok(())
}

async fn roll_dice(ctx: &Context<'_>, nb_faces: u32) -> Result<u32> {
    let result = thread_rng().gen_range(1..=nb_faces);
    let mut current_roll_session_writer =
                    // SAFETY: at this point, `CURRENT_ROLL_SESSION_WRITER` is initialized
                    unsafe { CURRENT_ROLL_SESSION_WRITER.get().unwrap_unchecked() }.lock().await;
    current_roll_session_writer
        .write_record([
            ctx.author().id.to_string(),
            result.to_string(),
            nb_faces.to_string(),
            ctx.created_at().to_string(),
        ])
        .expect("Could not write a record in the current session");
    Ok(result)
}

/// Jette des dés.
///
/// Exemples :
/// * `/r 1d100`
/// * `/r 5d6`
/// * `/r 1d3 + (3d4 * 2d20)`
#[command(prefix_command, aliases("r"), category = "Dés")]
pub async fn roll(ctx: Context<'_>, #[rest] expr: String) -> Result<()> {
    let exp = match Calculator::parse(Rule::expr, &expr) {
        Ok(pair) => parse_expr(pair)?,
        Err(err) => return Err(anyhow!("Erreur dans le parsing: {err}")),
    };

    let ast = exp.eval(&ctx).await?;

    if let Expr::Dices(rolls) = &ast {
        let sent_message = ctx.reply(format!("{}\n> {}", ctx.author().name, ast)).await?;

        if rolls.len() == 1 && rolls[0] == 69 {
            for emoji in NICE {
                sent_message.message().await?.react(ctx.http(), emoji).await?;
            }
        }
    } else {
        ctx.reply(format!("{}\n> {}\nTotal : {}", ctx.author().name, ast, ast.value())).await?;
    }

    Ok(())
}

/// Jette deux fois les mêmes dés.
///
/// Exemples :
/// * `/rr 1d100`
/// * `/rr 5d6`
/// * `/rr 1d3 + (3d4 * 2d20)`
#[command(prefix_command, aliases("rr"), category = "Dés")]
pub async fn roll_twice(ctx: Context<'_>, nb_jets: Option<u32>, #[rest] expr: String) -> Result<()> {
    let exp = match Calculator::parse(Rule::expr, &expr) {
        Ok(pair) => parse_expr(pair)?,
        Err(err) => return Err(anyhow!("Erreur dans le parsing: {err}")),
    };

    let mut acc = String::new();
    acc.push_str(&ctx.author().name);

    for i in 1..=nb_jets.unwrap_or(2) {
        let ast = exp.eval(&ctx).await?;
        acc.push_str(&format!("\nJet {i} :\n > {}\nTotal : {}", ast, ast.value()));
    }

    ctx.reply(acc).await?;

    Ok(())
}

/// Crée une nouvelle session de jeu.
#[command(prefix_command, category = "Dés")]
pub async fn session(ctx: Context<'_>) -> Result<()> {
    new_session().await?;
    ctx.say("Une nouvelle session vient de débuter !").await?;
    Ok(())
}

fn draw_boxplot(rolls: &Rolls, title: &str, nb_faces: Option<u32>) -> Result<NamedTempFile> {
    const WIDTH: u32 = 640;
    const LENGTH: u32 = 480;

    let file = tempfile::Builder::new().suffix(".png").tempfile()?;

    {
        let root = BitMapBackend::new(file.path(), (WIDTH, LENGTH)).into_drawing_area();
        root.fill(&WHITE)?;

        let mut chart_builder = ChartBuilder::on(&root);
        chart_builder.margin(5).set_left_and_bottom_label_area_size(20);

        let mut datas = HashMap::new();
        for roll in &rolls.0 {
            *datas
                .entry(if nb_faces.is_some() {
                    (roll.result / (roll.sides / 10)) * (roll.sides / 10) + (roll.sides / 20)
                } else {
                    5 + (((roll.result * 100) as f32 / roll.sides as f32 / 10_f32) as u32) * 10
                })
                .or_insert(0) += 1;
        }

        let mut chart_context = chart_builder
            .caption(title, ("sans-serif", 25).into_font())
            .build_cartesian_2d(
                (0_u32..nb_faces.unwrap_or(100_u32)).into_segmented(),
                0_u32..*datas.values().max().ok_or(anyhow!("Cannot create a graph from an empty set of rolls"))?,
            )
            .unwrap();
        chart_context.configure_mesh().draw().unwrap();
        chart_context
            .draw_series(Histogram::vertical(&chart_context).style(BLUE.filled()).margin(20).data(datas))
            .unwrap();

        root.present()?;
    }

    Ok(file)
}

/// Affiche des statistiques relatives aux jets de dés
///
/// Exemples :
/// * `/stats d100`
/// * `/stats * @user`
/// * `/stats * d100`
/// * `/stats @user d50`
#[command(prefix_command, category = "Dés")]
pub async fn stats(ctx: Context<'_>, args: Vec<String>) -> Result<()> {
    // SAFETY: at this point, `CURRENT_ROLL_SESSION_WRITER` is set
    unsafe { CURRENT_ROLL_SESSION_WRITER.get().unwrap_unchecked() }.lock().await.flush()?;

    let mut is_all_rolls = false;
    let mut faces_per_dice_opt: Option<u32> = None;
    let mut user_id_opt: Option<UserId> = None;

    for arg in args.iter() {
        if arg.is_empty() {
            error!("L'argument donné est vide");
        } else if *arg == "*" {
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
            user_id_opt = Some(UserId::new(u64::from_str(chars.as_str())?));
        } else {
            error!("Argument \"{}\" invalide", arg);
        }
    }

    let sessions = if is_all_rolls {
        let sessions_file_path = Path::join(&ROLL_DIR, "sessions.txt");
        let content = fs::read_to_string(sessions_file_path)?;
        let mut sessions = content.split('\n').map(ToOwned::to_owned).collect::<Vec<String>>();
        sessions.pop();
        sessions
    } else {
        let binding = CURRENT_ROLL_SESSION.lock().await;
        vec![<String as AsRef<str>>::as_ref(&binding).to_owned()]
    };

    let rolls = Rolls(
        sessions
            .into_iter()
            .flat_map(|session| load_session(&session).unwrap_or_else(|_| panic!("Could not load session {}", session)))
            .filter(|roll| match (faces_per_dice_opt, user_id_opt) {
                (None, None) => true,
                (None, Some(user_id)) => roll.user_id == user_id.get(),
                (Some(faces_per_dice), None) => roll.sides == faces_per_dice,
                (Some(faces_per_dice), Some(user_id)) => roll.sides == faces_per_dice && roll.user_id == user_id.get(),
            })
            .collect::<Vec<Roll>>(),
    );

    match (faces_per_dice_opt, user_id_opt) {
        (None, None) => {
            ctx.say(format!(
                "Sur un total de {} dés lancés, après normalisation, la moyenne est de {} et l'écart-type de {}.",
                rolls.len(),
                f64::trunc(rolls.normalized_avg() * 1000_f64) / 1000_f64,
                f64::trunc(rolls.normalized_sd() * 1000_f64) / 1000_f64
            ))
            .await?
        },
        (Some(faces_per_dice), None) => {
            ctx.say(format!(
                "Sur un total de {} de d{} lancés, la moyenne est de {} et l'écart-type de {}.",
                rolls.len(),
                faces_per_dice,
                f64::trunc(rolls.avg() * 1000_f64) / 1000_f64,
                f64::trunc(rolls.sd() * 1000_f64) / 1000_f64
            ))
            .await?
        },
        (None, Some(user_id)) => {
            ctx.say(format!(
                "Sur un total de {} dés lancés par <@{}>, après normalisation la moyenne est de {} et l'écart-type de {}.",
                rolls.len(),
                user_id,
                f64::trunc(rolls.normalized_avg() * 1000_f64) / 1000_f64,
                f64::trunc(rolls.normalized_sd() * 1000_f64) / 1000_f64
            ))
            .await?
        },
        (Some(faces_per_dice), Some(user_id)) => {
            ctx.say(format!(
                "Sur un total de {} de d{} lancés par <@{}>, la moyenne est de {} et l'écart-type de {}.",
                rolls.len(),
                faces_per_dice,
                user_id,
                f64::trunc(rolls.avg() * 1000_f64) / 1000_f64,
                f64::trunc(rolls.sd() * 1000_f64) / 1000_f64
            ))
            .await?
        },
    };

    let named_file = draw_boxplot(
        &rolls,
        &format!(
            "jets de {} {}",
            match user_id_opt {
                None => "tout le monde".to_owned(),
                Some(user_id) => user_id.to_user(ctx.http()).await?.name,
            },
            match faces_per_dice_opt {
                None => "normalisés".to_owned(),
                Some(faces) => format!("sur des d{}", faces),
            },
        ),
        faces_per_dice_opt,
    )?;
    ctx.channel_id()
        .send_files(ctx.http(), [CreateAttachment::path(named_file.path()).await?], CreateMessage::new().content(""))
        .await?;

    Ok(())
}
