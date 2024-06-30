//! Commands used to draw playing card

use std::fmt::Display;

use anyhow::{anyhow, Error, Result};
use poise::command;
use rand::distributions::{Distribution, Standard};
use rand::{random, Rng};

use crate::Context;

/// Possible playing cart suits
#[derive(Debug)]
enum Suit {
    /// ♣
    Club,

    /// ♥
    Heart,

    /// ♠
    Spade,

    /// ♦
    Diamond,
}

impl TryFrom<u32> for Suit {
    type Error = Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Club),
            1 => Ok(Self::Heart),
            2 => Ok(Self::Spade),
            3 => Ok(Self::Diamond),
            _ => Err(anyhow!("Invalid suit")),
        }
    }
}

impl Display for Suit {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(fmt, "{}", match self {
            Self::Club => "Trèfle",
            Self::Heart => "Cœur",
            Self::Spade => "Pique",
            Self::Diamond => "Carreau",
        })
    }
}

impl Suit {
    const fn one_char(&self) -> &'static str {
        match self {
            Self::Club => "C",
            Self::Heart => "H",
            Self::Spade => "S",
            Self::Diamond => "D",
        }
    }
}

/// Possible card values
#[derive(Debug)]
enum Value {
    /// A
    Ace,

    /// 2
    Two,

    /// 3
    Three,

    /// 4
    Four,

    /// 5
    Five,

    /// 6
    Six,

    /// 7
    Seven,

    /// 8
    Eight,

    /// 9
    Nine,

    /// 10
    Ten,

    /// J
    Jack,

    /// Q
    Queen,

    /// K
    King,
}

impl TryFrom<u32> for Value {
    type Error = Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Ace),
            2 => Ok(Self::Two),
            3 => Ok(Self::Three),
            4 => Ok(Self::Four),
            5 => Ok(Self::Five),
            6 => Ok(Self::Six),
            7 => Ok(Self::Seven),
            8 => Ok(Self::Eight),
            9 => Ok(Self::Nine),
            10 => Ok(Self::Ten),
            11 => Ok(Self::Jack),
            12 => Ok(Self::Queen),
            13 => Ok(Self::King),
            _ => Err(anyhow!("Invalid value")),
        }
    }
}

impl Display for Value {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(fmt, "{}", match self {
            Self::Ace => "As",
            Self::Two => "2",
            Self::Three => "3",
            Self::Four => "4",
            Self::Five => "5",
            Self::Six => "6",
            Self::Seven => "7",
            Self::Eight => "8",
            Self::Nine => "9",
            Self::Ten => "10",
            Self::Jack => "Valet",
            Self::Queen => "Dame",
            Self::King => "Roi",
        })
    }
}

impl Value {
    const fn one_char(&self) -> &'static str {
        match self {
            Self::Ace => "A",
            Self::Two => "2",
            Self::Three => "3",
            Self::Four => "4",
            Self::Five => "5",
            Self::Six => "6",
            Self::Seven => "7",
            Self::Eight => "8",
            Self::Nine => "9",
            Self::Ten => "0",
            Self::Jack => "J",
            Self::Queen => "Q",
            Self::King => "K",
        }
    }
}

/// Possible playing cards
#[derive(Debug)]
enum Card {
    /// Joker card
    Joker,

    /// Normal card
    Normal(Value, Suit),
}

impl Distribution<Card> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Card {
        match rng.gen_range(1_u32..=54) {
            i @ 1..=52 => Card::Normal(Value::try_from((i - 1) % 13 + 1).unwrap(), Suit::try_from((i - 1) / 13).unwrap()),
            53..=54 => Card::Joker,
            _ => unreachable!(),
        }
    }
}

impl Display for Card {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(fmt, "{}", match self {
            Self::Joker => "Joker".to_owned(),
            Self::Normal(value, suit) => format!("{value} de {suit}"),
        })
    }
}

impl Card {
    fn two_chars(&self) -> String {
        match self {
            Card::Joker => "XX".to_owned(),
            Card::Normal(value, suit) => format!("{}{}", value.one_char(), suit.one_char()),
        }
    }
}

/// Pioche une carte au hasard parmi un jeu de 54 cartes (52 usuelles + 2 jokers)
#[command(prefix_command, aliases("carte", "card", "pioche"), category = "Cartes")]
pub async fn draw(ctx: Context<'_>) -> Result<()> {
    let card: Card = random();
    ctx.say(format!("{}\n> {card}", ctx.author().name)).await?;
    ctx.say(format!("https://www.deckofcardsapi.com/static/img/{}.png", card.two_chars()))
        .await?;
    Ok(())
}
