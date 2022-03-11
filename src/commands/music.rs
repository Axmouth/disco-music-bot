use dashmap::DashMap;
use serenity::framework::standard::{macros::command, CommandResult};
use serenity::framework::standard::{Args, CommandError};
use serenity::model::prelude::*;
use serenity::Result as SerenityResult;
use serenity::{async_trait, prelude::*};
use songbird::input::Input;
use songbird::tracks::TrackHandle;
use songbird::{EventContext, EventHandler, Songbird, TrackEvent};
use std::sync::Arc;

#[derive(Debug)]
pub struct QueuedSong {
    name: String,
    input: Input,
}

#[derive(Debug)]
pub enum PlayingStatus {
    Playing { name: String },
    Paused { name: String },
    Stopped,
}

impl Default for PlayingStatus {
    fn default() -> Self {
        PlayingStatus::Stopped
    }
}

#[derive(Debug)]
pub struct GuildMusicState {
    guild_id: GuildId,
    queue: Vec<QueuedSong>,
    playing_status: PlayingStatus,
    handle: Option<TrackHandle>,
    manager: Arc<Songbird>,
}

#[derive(Debug, Clone, Default)]
pub struct MusicState {
    pub guild_states: DashMap<GuildId, Arc<Mutex<GuildMusicState>>>,
}

impl TypeMapKey for MusicState {
    type Value = MusicState;
}

pub struct SongFinishedEventHandler(pub Arc<Mutex<GuildMusicState>>);

#[async_trait]
impl EventHandler for SongFinishedEventHandler {
    async fn act(&self, _: &EventContext<'_>) -> Option<songbird::Event> {
        println!("Song finished");
        let mut state = self.0.lock().await;

        let next = state.queue.pop();

        if let Some(next_song) = next {
            state.playing_status = PlayingStatus::Playing {
                name: next_song.name,
            };

            if let Some(handler_lock) = state.manager.get(state.guild_id) {
                let mut handler = handler_lock.lock().await;
                handler.stop();
                let handle = handler.play_source(next_song.input);
                handle
                    .add_event(
                        songbird::Event::Track(TrackEvent::End),
                        SongFinishedEventHandler(self.0.clone()),
                    )
                    .map_err(|e| println!("Failed to add event : {e}"))
                    .ok();
                state.handle = Some(handle);
            }
        } else {
            state.playing_status = PlayingStatus::Stopped;
        }

        None
    }
}

pub struct SongStartEventHandler(pub Arc<Mutex<GuildMusicState>>);

#[async_trait]
impl EventHandler for SongStartEventHandler {
    async fn act(&self, _: &EventContext<'_>) -> Option<songbird::Event> {
        None
    }
}

pub struct SongPauseEventHandler(pub Arc<Mutex<GuildMusicState>>);

#[async_trait]
impl EventHandler for SongPauseEventHandler {
    async fn act(&self, _: &EventContext<'_>) -> Option<songbird::Event> {
        None
    }
}

#[command]
async fn joinchan(ctx: &Context, msg: &Message) -> CommandResult {
    let user_id = msg.author.id;

    let guild = msg
        .guild(ctx.cache.clone())
        .await
        .ok_or(CommandError::from("No guild found."))?;

    let channel_id = guild
        .voice_states
        .get(&user_id)
        .map(|state| state.channel_id)
        .flatten()
        .ok_or(CommandError::from("No channel found."))?;

    let guild = msg.guild(&ctx.cache).await.unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    let _ = manager.join(guild_id, channel_id).await;

    Ok(())
}

#[command]
async fn play(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let user_id = msg.author.id;

    let guild = msg.guild(&ctx.cache).await.expect("Guild not found.");

    let channel_id = guild
        .voice_states
        .get(&user_id)
        .map(|state| state.channel_id)
        .flatten()
        .ok_or(CommandError::from("No channel found."))?;

    let url = match args.single::<String>() {
        Ok(url) => url,
        Err(_) => {
            check_msg(
                msg.channel_id
                    .say(&ctx.http, "No video or audio provided, playing default")
                    .await,
            );

            "https://youtube.com/watch?v=dQw4w9WgXcQ".to_owned()
        }
    };

    if !url.starts_with("http") {
        check_msg(
            msg.channel_id
                .say(&ctx.http, "Must provide a valid URL")
                .await,
        );

        return Ok(());
    }

    let guild_id = guild.id;

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();
    let _ = manager.join(guild_id, channel_id).await;

        let mut ctx_data = ctx.data.write().await;
        let music_states = if let Some(music_states) = ctx_data.get_mut::<MusicState>() {
            music_states
        } else {
            ctx_data.insert::<MusicState>(MusicState::default());
            ctx_data
                .get_mut::<MusicState>()
                .expect("MusicState not found")
        };

    if let Some(handler_lock) = manager.get(guild_id) {
        let mut handler = handler_lock.lock().await;

        let input = match songbird::ytdl(&url).await {
            Ok(source) => source,
            Err(why) => {
                println!("Err starting source: {:?}", why);

                check_msg(
                    msg.channel_id
                        .say(&ctx.http, format!("Error sourcing ffmpeg : {why}"))
                        .await,
                );

                return Ok(());
            }
        };


        println!("Got music states");
        if let Some(music_state_mutex) = music_states.guild_states.get_mut(&guild_id) {
            println!("Got music state 2");
            let mut state = music_state_mutex.lock().await;
            if let PlayingStatus::Playing { .. } = state.playing_status {
                println!("Got music state 3");

                check_msg(
                    msg.channel_id
                        .say(
                            &ctx.http,
                            format!("Queing <{url}> ({} tracks in queue)", state.queue.len() + 1),
                        )
                        .await,
                );
                state.queue.push(QueuedSong { name: url, input });
            } else {
                println!("Got music state 4");
                check_msg(
                    msg.channel_id
                        .say(&ctx.http, format!("Playing <{url}>"))
                        .await,
                );
                state.playing_status = PlayingStatus::Playing { name: url };
                let handle = handler.play_source(input);
                handle
                    .add_event(
                        songbird::Event::Track(TrackEvent::End),
                        SongFinishedEventHandler(music_state_mutex.clone()),
                    )
                    .map_err(|e| CommandError::from(format!("Failed to add event : {e}")))?;
                state.handle = Some(handle);
                state.manager = manager;
            }
        } else {
            println!("Got music state 5");
            let queue = Vec::new();
            let handle = handler.play_source(input);
            println!("Got music state 6");
            check_msg(
                msg.channel_id
                    .say(&ctx.http, format!("Playing <{url}>"))
                    .await,
            );
            let music_state_mutex = Arc::new(Mutex::new(GuildMusicState {
                guild_id,
                queue,
                playing_status: PlayingStatus::Playing { name: url },
                handle: None,
                manager,
            }));
            println!("Got music state 7");
            handle
                .add_event(
                    songbird::Event::Track(TrackEvent::End),
                    SongFinishedEventHandler(music_state_mutex.clone()),
                )
                .map_err(|e| CommandError::from(format!("Failed to add event : {e}")))?;
            println!("Got music state 8");
            music_state_mutex.lock().await.handle = Some(handle);
            println!("Got music state 9");
            music_states
                .guild_states
                .insert(guild_id, music_state_mutex.clone());
        }

        let _ = &ctx;
    } else {
        check_msg(
            msg.channel_id
                .say(&ctx.http, "Not in a voice channel to play in")
                .await,
        );
    }

    println!("Done");

    Ok(())
}

#[command]
async fn stop(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).await.unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    let _ = manager.leave(guild_id).await;

    Ok(())
}

#[command]
async fn skip(ctx: &Context, msg: &Message, _: Args) -> CommandResult {
    let guild = msg.guild(&ctx.cache).await.expect("Guild not found.");
    let guild_id = guild.id;
    let mut ctx_data = ctx.data.write().await;
    let music_states = if let Some(music_states) = ctx_data.get_mut::<MusicState>() {
        music_states
    } else {
        ctx_data.insert::<MusicState>(MusicState::default());
        ctx_data
            .get_mut::<MusicState>()
            .expect("MusicState not found")
    };
    let queue_len = if let Some(music_state_mutex) = music_states.guild_states.get_mut(&guild_id) {
        let state = music_state_mutex.lock().await;

        if let Some(handle) = &state.handle {
            handle
                .stop()
                .map_err(|e| CommandError::from(format!("Failed to pause : {e}")))?;

        }
        state.queue.len()
    } else {
        return Ok(());
    };

    if queue_len > 0 {
        check_msg(
            msg.channel_id
                .say(
                    &ctx.http,
                    format!("Skipping ({} tracks in queue)", queue_len - 1),
                )
                .await,
        );
    } else {
        check_msg(
            msg.channel_id
                .say(&ctx.http, "Skipping, no more tracks to play")
                .await,
        );
    }

    Ok(())
}

#[command]
async fn pause(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).await.expect("Guild not found.");
    let guild_id = guild.id;
    let mut ctx_data = ctx.data.write().await;
    let music_states = if let Some(music_states) = ctx_data.get_mut::<MusicState>() {
        music_states
    } else {
        ctx_data.insert::<MusicState>(MusicState::default());
        ctx_data
            .get_mut::<MusicState>()
            .expect("MusicState not found")
    };
    if let Some(music_state_mutex) = music_states.guild_states.get_mut(&guild_id) {
        let mut state = music_state_mutex.lock().await;
        if let PlayingStatus::Playing { name } = &state.playing_status {
            state.playing_status = PlayingStatus::Paused { name: name.clone() };
            check_msg(msg.channel_id.say(&ctx.http, "Pausing").await);
        } else {
            check_msg(
                msg.channel_id
                    .say(&ctx.http, "Not playing so can't pause")
                    .await,
            );
        }
        if let Some(handle) = &state.handle {
            handle
                .pause()
                .map_err(|e| CommandError::from(format!("Failed to pause : {e}")))?;
        }
    }

    Ok(())
}

#[command]
async fn unpause(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).await.expect("Guild not found.");
    let guild_id = guild.id;
    let mut ctx_data = ctx.data.write().await;
    let music_states = if let Some(music_states) = ctx_data.get_mut::<MusicState>() {
        music_states
    } else {
        ctx_data.insert::<MusicState>(MusicState::default());
        ctx_data
            .get_mut::<MusicState>()
            .expect("MusicState not found")
    };
    if let Some(music_state_mutex) = music_states.guild_states.get_mut(&guild_id) {
        let mut state = music_state_mutex.lock().await;
        if let PlayingStatus::Paused { name } = &state.playing_status {
            state.playing_status = PlayingStatus::Playing { name: name.clone() };
            check_msg(msg.channel_id.say(&ctx.http, "Unpausing").await);
        } else {
            check_msg(
                msg.channel_id
                    .say(&ctx.http, "Not paused so can't unpause")
                    .await,
            );
        }
        if let Some(handle) = &state.handle {
            handle
                .play()
                .map_err(|e| CommandError::from(format!("Failed to pause : {e}")))?;
        }
    }

    Ok(())
}

#[command]
async fn quit(ctx: &Context, msg: &Message, _: Args) -> CommandResult {
    let guild = msg.guild(&ctx.cache).await.unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    let _ = manager.leave(guild_id).await;

    Ok(())
}

/// Checks that a message successfully sent; if not, then logs why to stdout.
fn check_msg(result: SerenityResult<Message>) {
    if let Err(why) = result {
        println!("Error sending message: {:?}", why);
    }
}