use std::env;
use std::time::Duration;

use dotenv::dotenv;
use once_cell::sync::Lazy;
use redis::{Commands, SetExpiry, SetOptions};
use regex::Regex;
use serenity::all::{
    ChannelId, CreateButton, CreateEmbed, CreateEmbedFooter, CreateMessage, GuildId, ReactionType,
};
use serenity::futures::StreamExt;
use serenity::model::channel::Message;
use serenity::prelude::*;
use serenity::{all::MessageId, async_trait};
use std::hash::{DefaultHasher, Hash, Hasher};

struct Handler {
    redis: redis::Client,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message_delete(
        &self,
        _ctx: Context,
        channel_id: ChannelId,
        deleted_message_id: MessageId,
        _guild_id: Option<GuildId>,
    ) {
        let meta = format!("{}-{}", channel_id.get(), deleted_message_id);
        let existing: Result<String, redis::RedisError> =
            self.redis.get_connection().unwrap().get(&meta);

        if let Ok(existing) = existing {
            let _: Result<String, redis::RedisError> =
                self.redis.get_connection().unwrap().del(existing);
            let _: Result<String, redis::RedisError> =
                self.redis.get_connection().unwrap().del(meta);
        }
    }

    async fn message(&self, ctx: Context, msg: Message) {
        // Ignore messages from bots.
        if msg.author.bot {
            return;
        }
        static RE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(http|ftp|https:)\/\/([\w_-]+(?:(?:\.[\w_-]+)+)[\w.,@?^=%&:\/~+#-]*[\w@?^=%&\/~+#-])").unwrap()
        });
        let mut links = vec![];
        for (link, [_, _]) in RE.captures_iter(&msg.content).map(|c| c.extract()) {
            links.push(link);
        }
        // Remove any links that reference media.
        for embed in &msg.embeds {
            let e = embed.clone();
            if vec![
                "image".to_owned(),
                "video".to_owned(),
                "gifv".to_owned(),
                "rich".to_owned(),
            ]
            .contains(&e.kind.unwrap_or_else(|| "".to_owned()))
            {
                let url = e.url.unwrap();
                links.retain(|x| *x != url);
            }
        }
        if links.len() > 0 {
            let opts = SetOptions::default()
                .with_expiration(SetExpiry::EX(60 * 60 * 24 * 7));
            let mut exists = vec![];
            let channel_id = msg.channel_id.get();
            for link in links {
                let mut s = DefaultHasher::new();
                format!("{}-{}", link, channel_id).hash(&mut s);
                let h = s.finish();
                let hash = format!("{:x}", h);
                let meta = format!("{}-{}", channel_id, msg.id.get());
                let existing: Result<String, redis::RedisError> =
                    self.redis.get_connection().unwrap().get(&hash);
                if existing.is_ok() {
                    exists.push((link, existing.unwrap()));
                } else {
                    let _: () = self
                        .redis
                        .get_connection()
                        .unwrap()
                        .set_options(&hash, &msg.link(), opts.clone())
                        .unwrap();
                    let _: () = self
                        .redis
                        .get_connection()
                        .unwrap()
                        .set_options(meta, hash, opts)
                        .unwrap();
                }
            }
            if exists.len() > 0 {
                // Links have already been posted, let's tell them
                let desc = format!(
                    "{} been posted in the last 7 days in this channel",
                    if exists.len() > 1 {
                        "Some of these have"
                    } else {
                        "One of these has"
                    }
                );
                let mut fields = vec![];
                for existing in exists {
                    fields.push((existing.0, existing.1, true));
                }
                let footer = CreateEmbedFooter::new("Bugs? Ask @gmem.ca");
                let embed = CreateEmbed::new()
                    .title("Duplicate Links")
                    .description(desc)
                    .fields(fields)
                    .footer(footer);
                let ignore_emoji: ReactionType = "🗑".parse().unwrap();
                let remove_emoji: ReactionType = "👍".parse().unwrap();
                let builder = CreateMessage::new()
                    .embed(embed)
                    .button(
                        CreateButton::new("ignore")
                            .label("Ignore")
                            .emoji(ignore_emoji),
                    )
                    .button(
                        CreateButton::new("remove")
                            .label("Remove my post")
                            .emoji(remove_emoji),
                    );
                let reply = msg
                    .channel_id
                    .send_message(&ctx.http, builder)
                    .await
                    .unwrap();

                // Wait for multiple interactions
                let mut interaction_stream = reply
                    .await_component_interaction(&ctx.shard)
                    .timeout(Duration::from_secs(60 * 3))
                    .stream();

                while let Some(interaction) = interaction_stream.next().await {
                    if interaction.user.id == msg.author.id {
                        let action = &interaction.data.custom_id;
                        if action == "remove" {
                            msg.delete(&ctx).await.unwrap();
                        }
                        reply.delete(&ctx).await.unwrap();
                    }
                }
            }
        }
    }
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    let redis_url = env::var("REDIS_URL").expect("Expected a Redis instance URL");
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    // Set gateway intents, which decides what events the bot will be notified about
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let client = redis::Client::open(redis_url).unwrap();
    let handler = Handler { redis: client };

    // Create a new instance of the Client, logging in as a bot.
    let mut client = Client::builder(&token, intents)
        .event_handler(handler)
        .await
        .expect("Err creating client");

    // Start listening for events by starting a single shard
    if let Err(why) = client.start_autosharded().await {
        println!("Client error: {why:?}");
    }
}
