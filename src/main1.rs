//! Requires the 'framework' feature flag be enabled in your project's
//! `Cargo.toml`.
//!
//! This can be enabled by specifying the feature in the dependency section:
//!
//! ```toml
//! [dependencies.serenity]
//! git = "https://github.com/serenity-rs/serenity.git"
//! features = ["framework", "standard_framework"]
//! ```
mod commands;
mod redis_store;
mod util;

use std::{collections::HashSet, env, sync::Arc};

use commands::{meta::*, music::*, owner::*};
use redis_store::RedisStore;
use serenity::{
    async_trait,
    client::bridge::gateway::ShardManager,
    framework::{standard::macros::group, StandardFramework},
    http::Http,
    model::{event::ResumedEvent, gateway::Ready},
    prelude::*,
};
use songbird::SerenityInit;
use tracing::{error, info};

pub struct DiscordTokenContainer(pub String);

impl TypeMapKey for DiscordTokenContainer {
    type Value = String;
}

impl DiscordTokenContainer {
    pub fn get(&self) -> &str {
        &self.0
    }
}

pub struct RedisClientContainer(pub redis::Client);

impl TypeMapKey for RedisClientContainer {
    type Value = redis::Client;
}

impl RedisClientContainer {
    pub fn get(&self) -> &redis::Client {
        &self.0
    }
}

pub struct ShardManagerContainer;

impl TypeMapKey for ShardManagerContainer {
    type Value = Arc<Mutex<ShardManager>>;
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        info!("Connected as {}", ready.user.name);
    }

    async fn resume(&self, _: Context, _: ResumedEvent) {
        info!("Resumed");
    }
}

// TODO: Add help command
#[group]
#[commands(
    prefix, ping, quit1, joinchan, pause, play, search, stop, skip, queue, quit, unpause
)]
struct General;

#[tokio::main]
async fn main() {
    dotenv::dotenv().expect("Failed to load .env file");

    tracing_subscriber::fmt::init();

    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    let redis_client = redis::Client::open(
        env::var("REDIS_URL").expect("Expected a redis url in the environment"),
    )
    .expect("Failed to connect to Redis");

    let http = Http::new_with_token(&token);

    // We will fetch your bot's owners and id
    let (owners, _bot_id) = match http.get_current_application_info().await {
        Ok(info) => {
            let mut owners = HashSet::new();
            owners.insert(info.owner.id);

            (owners, info.id)
        }
        Err(why) => panic!("Could not access application info: {:?}", why),
    };

    // Create the framework
    let framework = StandardFramework::new()
        .configure(|c| {
            c.owners(owners)
                .dynamic_prefix(|ctx, msg| {
                    Box::pin(async {
                        match ctx
                            .data
                            .read()
                            .await
                            .get::<RedisClientContainer>()
                            .expect("Failed to get RedisClientContainer")
                            .get_async_connection()
                            .await
                        {
                            Ok(conn) => match msg.guild_id {
                                Some(guild_id) => {
                                    let prefix = RedisStore::new(conn).get_prefix(guild_id).await;
                                    match prefix {
                                        Ok(prefix) => prefix,
                                        Err(_) => None,
                                    }
                                }
                                None => None,
                            },
                            Err(_) => None,
                        }
                    })
                })
                .prefix("~")
        })
        .group(&GENERAL_GROUP);

    let mut client = Client::builder(&token)
        .framework(framework)
        .event_handler(Handler)
        .register_songbird()
        .await
        .expect("Err creating client");

    {
        let mut data = client.data.write().await;
        data.insert::<ShardManagerContainer>(client.shard_manager.clone());
        data.insert::<DiscordTokenContainer>(token);
        data.insert::<RedisClientContainer>(redis_client);
    }

    let shard_manager = client.shard_manager.clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Could not register ctrl+c handler");
        shard_manager.lock().await.shutdown_all().await;
    });

    if let Err(why) = client.start().await {
        error!("Client error: {:?}", why);
    }
}
