use serenity::framework::standard::Args;
use serenity::framework::standard::{macros::command, CommandResult};
use serenity::model::prelude::*;
use serenity::prelude::*;

use crate::redis_store::RedisStore;
use crate::RedisClientContainer;

#[command]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    msg.channel_id.say(&ctx.http, "Pong!").await?;

    Ok(())
}

#[command]
async fn prefix(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let data = ctx.data.read().await;
    let redis_client = match data.get::<RedisClientContainer>() {
        Some(redis_client) => redis_client,
        None => {
            msg.channel_id
                .say(&ctx.http, "Redis client not found")
                .await?;
            return Ok(());
        }
    };

    if let (Ok("set"), Ok(new_prefix), true) = (
        args.single::<String>().as_deref(),
        args.single_quoted::<String>(),
        args.is_empty(),
    ) {
        let guild_id = match msg.guild_id {
            Some(guild_id) => guild_id,
            None => {
                msg.channel_id
                    .say(&ctx.http, "This command can only be used in a guild")
                    .await?;
                return Ok(());
            }
        };

        let conn = match redis_client.get_async_connection().await {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("Failed to get Redis connection : {}", e);
                msg.channel_id
                    .say(&ctx.http, "Failed to get Redis connection")
                    .await?;
                return Ok(());
            }
        };

        let mut redis_store = RedisStore::new(conn);
        match redis_store.set_prefix(guild_id, &new_prefix).await {
            Ok(_) => {
                msg.channel_id
                    .say(&ctx.http, format!("Prefix set to `{new_prefix}`"))
                    .await?;
            }
            Err(e) => {
                eprintln!("Failed to set prefix : {:?}", e);
                msg.channel_id
                    .say(&ctx.http, "Failed to set prefix")
                    .await?;
            }
        }

        return Ok(());
    }

    match msg.guild_id {
        Some(guild_id) => {
            let conn = match redis_client.get_async_connection().await {
                Ok(conn) => conn,
                Err(e) => {
                    eprintln!("Failed to get Redis connection : {}", e);
                    msg.channel_id
                        .say(&ctx.http, "Failed to get Redis connection")
                        .await?;
                    return Ok(());
                }
            };
            let prefix = RedisStore::new(conn).get_prefix(guild_id).await;
            match prefix {
                Ok(Some(prefix)) => msg.channel_id.say(&ctx.http, &prefix).await?,
                Ok(None) => msg.channel_id.say(&ctx.http, "No prefix set").await?,
                Err(e) => {
                    eprintln!("Failed to get prefix : {:?}", e);
                    msg.channel_id
                        .say(&ctx.http, "Failed to get prefix")
                        .await?
                }
            };
        }
        None => {
            msg.channel_id
                .say(&ctx.http, "This command can only be used in a guild")
                .await?;
        }
    }

    Ok(())
}
