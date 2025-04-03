//! Commands related to the Cthulhu Dark RPG.

use poise::command;
use rand::{Rng, thread_rng};

use crate::{Context, Result};

/// Number of faces per dice in Cthulhu Dark.
const FACES_PER_DICE: u32 = 6;

/// Jette des dés pour le JdR Cthulhu Dark.
///
/// Exemples :
/// * `/rcd 10`
/// * `/rcd 20`
/// * `/rcd 21`
#[command(prefix_command, category = "CthulhuDark")]
pub async fn rcd(ctx: Context<'_>, rolls: u32) -> Result<()> {
    let human_dice_nb = rolls / 10;
    let insight_dice = rolls % 10 != 0;

    let mut human_rolls = Vec::new();
    for _ in 0..human_dice_nb {
        human_rolls.push(thread_rng().gen_range(1..=FACES_PER_DICE));
    }
    let insight_roll = if insight_dice { Some(thread_rng().gen_range(1..=FACES_PER_DICE)) } else { None };

    let mut messages = vec![];
    if !human_rolls.is_empty() {
        messages
            .push(format!("Résultat(s) : {}", human_rolls.iter().map(u32::to_string).collect::<Vec<_>>().join(" / ")));
    }
    if let Some(roll) = insight_roll {
        messages.push(format!("Perspicacité : {roll}"));
    }

    if messages.is_empty() {
        ctx.reply("Non mais au moins appelle-moi pour quelque chose d'utile :rage:").await?;
    } else {
        ctx.reply(messages.join("\n")).await?;
    }

    Ok(())
}
