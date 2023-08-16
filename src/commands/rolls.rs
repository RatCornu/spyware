use log::error;
use rand::{thread_rng, Rng};
use serenity::{
    framework::standard::{macros::command, Args, CommandResult, Delimiter},
    model::prelude::Message,
    prelude::Context,
};

const NICE: [char; 4] = ['🇳', '🇮', '🇨', '🇪'];

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
        let mut iter_results = results.into_iter();
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
            Ok(sent_message) if nb_dices == 1 && first_result == 69 => {
                for emoji in NICE {
                    sent_message.react(&ctx.http, emoji).await?;
                }
            },
            Ok(_) => {},
            Err(_) => {
                error!("Tried to roll {}d{} which is too large for one message", nb_dices, nb_faces);
                msg.channel_id.say(&ctx.http, "Le nombre de dés jetés est trop grand !").await?;
            },
        }
    }

    Ok(())
}
