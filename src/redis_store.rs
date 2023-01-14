use redis::{AsyncCommands, FromRedisValue, RedisError, ToRedisArgs};
use serenity::model::id::{ChannelId, GuildId};
use std::num::NonZeroUsize;

#[derive(Debug)]
pub enum RedisStoreError {
    RedisError(RedisError),
    InvalidKey,
    Deserialization(String),
}

impl From<RedisError> for RedisStoreError {
    fn from(err: RedisError) -> Self {
        RedisStoreError::RedisError(err)
    }
}

#[derive(Debug, Clone)]
pub enum PlayArgs {
    SearchQuery(String),
    YoutubeLink(String),
}

// TODO: tests?
impl PlayArgs {
    pub fn ser(&self) -> String {
        match self {
            PlayArgs::SearchQuery(query) => format!("search~{}", query),
            PlayArgs::YoutubeLink(link) => format!("youtube~{}", link),
        }
        .replace(':', "\\:")
    }

    pub fn deser(s: &str) -> Result<PlayArgs, RedisStoreError> {
        let args = s.split_once('~');
        match args {
            Some((cmd, query)) => match cmd {
                "search" => Ok(PlayArgs::SearchQuery(query.replace("\\:", ":"))),
                "youtube" => Ok(PlayArgs::YoutubeLink(query.replace("\\:", ":"))),
                _ => Err(RedisStoreError::Deserialization(format!(
                    "Unknown command: {}",
                    cmd
                ))),
            },
            None => Err(RedisStoreError::Deserialization(
                "No command found".to_string(),
            )),
        }
    }
}

#[derive(Debug)]
pub struct QueuedSong {
    channel_id: ChannelId,
    name: String,
    play: PlayArgs,
}

impl QueuedSong {
    pub fn ser(&self) -> String {
        format!(
            "{} :{} :{}",
            self.channel_id.0,
            self.name.replace(':', "\\:"),
            self.play.ser()
        )
    }

    pub fn deser(s: &str) -> Result<QueuedSong, RedisStoreError> {
        let args: Vec<&str> = s.splitn(3, " :").collect();
        match args.as_slice() {
            [channel_id, name, play] => Ok(QueuedSong {
                channel_id: ChannelId(channel_id.parse::<u64>().unwrap()),
                name: name.replace("\\:", ":"),
                play: PlayArgs::deser(play)?,
            }),
            _ => Err(RedisStoreError::Deserialization(
                "No channel_id found".to_string(),
            )),
        }
    }
}

fn prefix_key(guild_id: GuildId) -> String {
    format!("prefix:{}", guild_id.0)
}

fn queue_key(guild_id: GuildId) -> String {
    format!("queue:{}", guild_id.0)
}

pub struct RedisStore {
    conn: redis::aio::Connection,
}

impl RedisStore {
    pub fn new(conn: redis::aio::Connection) -> Self {
        Self { conn }
    }

    pub async fn get_prefix(
        &mut self,
        guild_id: GuildId,
    ) -> Result<Option<String>, RedisStoreError> {
        self.conn
            .get(prefix_key(guild_id))
            .await
            .map_err(RedisStoreError::RedisError)
    }

    pub async fn set_prefix(
        &mut self,
        guild_id: GuildId,
        prefix: &str,
    ) -> Result<(), RedisStoreError> {
        self.conn
            .set(prefix_key(guild_id), prefix)
            .await
            .map_err(RedisStoreError::RedisError)
    }

    pub async fn get_queue(
        &mut self,
        guild_id: GuildId,
    ) -> Result<Option<Vec<QueuedSong>>, RedisStoreError> {
        self.conn
            .lrange::<_, Option<Vec<String>>>(queue_key(guild_id), 0, -1)
            .await
            .map_err(RedisStoreError::RedisError)?
            .map(|v| {
                v.into_iter()
                    .map(|s| QueuedSong::deser(&s))
                    .collect::<Result<Vec<QueuedSong>, RedisStoreError>>()
            })
            .transpose()
    }

    pub async fn pop_queue(
        &mut self,
        guild_id: GuildId,
    ) -> Result<Option<QueuedSong>, RedisStoreError> {
        self.conn
            .lpop::<_, Option<String>>(queue_key(guild_id), None)
            .await
            .map_err(RedisStoreError::RedisError)?
            .as_deref()
            .map(QueuedSong::deser)
            .transpose()
    }

    pub async fn push_queue(
        &mut self,
        guild_id: GuildId,
        song: QueuedSong,
    ) -> Result<(), RedisStoreError> {
        self.conn
            .rpush(queue_key(guild_id), song.ser())
            .await
            .map_err(RedisStoreError::RedisError)
    }
}
