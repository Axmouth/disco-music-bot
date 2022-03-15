use std::num::NonZeroUsize;
use redis::{AsyncCommands, RedisError};
use serenity::model::id::GuildId;

pub struct RedisStore {
    conn: redis::aio::Connection,
}

impl RedisStore {
    pub fn new(conn: redis::aio::Connection) -> Self {
        Self { conn }
    }

    pub async fn get_prefix(&mut self, guild_id: GuildId) -> Result<Option<String>, RedisError> {
        self.conn.get(format!("prefix:{}", guild_id)).await
    }

    pub async fn set_prefix(&mut self, guild_id: GuildId, prefix: &str) -> Result<(), RedisError> {
        self.conn.set(format!("prefix:{}", guild_id), prefix).await
    }

    pub async fn get_queue(&mut self, guild_id: GuildId) -> Result<Option<Vec<String>>, RedisError> {
        self.conn.lrange(format!("queue:{}", guild_id), 0, -1).await
    }

    pub async fn pop_queue(&mut self, guild_id: GuildId, count: Option<NonZeroUsize>) -> Result<String, RedisError> {
        self.conn.lpop(format!("queue:{}", guild_id), count).await
    }

    pub async fn push_queue(&mut self, guild_id: GuildId, song: &str) -> Result<(), RedisError> {
        self.conn.rpush(format!("queue:{}", guild_id), song).await
    }
}
