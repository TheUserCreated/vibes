use std::{
    collections::{HashMap, HashSet},
    env,
    sync::Arc,
};

use dotenv;
use serenity::prelude::*;
use serenity::static_assertions::_core::sync::atomic::{AtomicUsize, Ordering};
use serenity::{
    async_trait,
    client::bridge::gateway::{GatewayIntents, ShardId, ShardManager},
    framework::standard::{
        buckets::{LimitedFor, RevertBucket},
        help_commands,
        macros::{check, command, group, help, hook},
        Args, CommandGroup, CommandOptions, CommandResult, DispatchError, HelpOptions, Reason,
        StandardFramework,
    },
    http::Http,
    model::{
        channel::{Channel, Message},
        gateway::Ready,
        id::UserId,
        interactions::{
            application_command::{
                ApplicationCommand, ApplicationCommandInteractionDataOptionValue,
                ApplicationCommandOptionType,
            },
            Interaction, InteractionResponseType,
        },
        permissions::Permissions,
        prelude::*,
    },
    utils::{content_safe, ContentSafeOptions},
};
use songbird::{
    input::{self, restartable::Restartable},
    Event, EventContext, EventHandler as VoiceEventHandler, SerenityInit, TrackEvent,
};
use tokio::sync::Mutex;
use tokio::time::Duration;

//Structures needed
//TODO: organize into a different file?
struct ShardManagerContainer;

impl TypeMapKey for ShardManagerContainer {
    type Value = Arc<Mutex<ShardManager>>;
}

struct TrackEndNotifier {
    chan_id: ChannelId,
    http: Arc<Http>,
}
#[async_trait]
impl VoiceEventHandler for TrackEndNotifier {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::Track(track_list) = ctx {}

        None
    }
}
#[async_trait]
impl VoiceEventHandler for ChannelDurationNotifier {
    async fn act(&self, _ctx: &EventContext<'_>) -> Option<Event> {
        let count_before = self.count.fetch_add(1, Ordering::Relaxed);

        None
    }
}

struct ChannelDurationNotifier {
    chan_id: ChannelId,
    count: Arc<AtomicUsize>,
    http: Arc<Http>,
}

struct CommandCounter;

impl TypeMapKey for CommandCounter {
    type Value = HashMap<String, u64>;
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
        let commands = ApplicationCommand::set_global_application_commands(&ctx.http, |commands| {
            commands
                .create_application_command(|command| command.name("ping").description("pong"))
                .create_application_command(|command| {
                    command
                        .name("id")
                        .description("Get a user id")
                        .create_option(|option| {
                            option
                                .name("id")
                                .description("The user to lookup")
                                .kind(ApplicationCommandOptionType::User)
                                .required(true)
                        })
                })
                .create_application_command(|command| {
                    command.name("join").description("Joins a Voice Channel.")
                })
        })
        .await;

        let guild_commands = GuildId(705038806893592657)
            .create_application_command(&ctx.http, |command| {
                command.name("test").description("for testing only")
            })
            .await;
        println!(
            "I now have the following global slash commands: {:#?}",
            commands
        );
    }
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            let content = match command.data.name.as_str() {
                "ping" => "Hey, I'm alive!".to_string(),
                "test" => "Test successful".to_string(),
                "join" => {
                    let executor = command.member.as_ref().expect("Cant be run outside guilds");
                    let guild = ctx.cache.guild(executor.guild_id).await.expect("failed");
                    let channel_id = guild
                        .voice_states
                        .get(&executor.user.id)
                        .and_then(|voice_state| voice_state.channel_id);
                    let connect_to = channel_id.expect("cant connect");
                    let manager = songbird::get(&ctx)
                        .await
                        .expect("songbird failed at init")
                        .clone();
                    let (handle_lock, success) = manager.join(guild.id, connect_to).await;
                    let chan_id = command.channel_id;
                    let send_http = ctx.http.clone();
                    let mut handle = handle_lock.lock().await;
                    handle.add_global_event(
                        Event::Track(TrackEvent::End),
                        TrackEndNotifier {
                            chan_id,
                            http: send_http,
                        },
                    );

                    let send_http = ctx.http.clone();
                    handle.add_global_event(
                        Event::Periodic(Duration::from_secs(60), None),
                        ChannelDurationNotifier {
                            chan_id,
                            count: Default::default(),
                            http: send_http,
                        },
                    );
                    "tried to join".to_string()
                }
                "id" => {
                    let options = command
                        .data
                        .options
                        .get(0)
                        .expect("Expected user option")
                        .resolved
                        .as_ref()
                        .expect("Expected user object");

                    if let ApplicationCommandInteractionDataOptionValue::User(user, _member) =
                        options
                    {
                        format!("{}'s id is {}", user.tag(), user.id)
                    } else {
                        "Please provide a valid user".to_string()
                    }
                }
                _ => "not implemented :(".to_string(),
            };

            if let Err(why) = command
                .create_interaction_response(&ctx.http, |response| {
                    response
                        .kind(InteractionResponseType::ChannelMessageWithSource)
                        .interaction_response_data(|message| message.content(content))
                })
                .await
            {
                println!("Cannot respond to slash command: {}", why);
            }
        }
    }
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().expect("Failed to load env file. Do you have one?");
    let token = env::var("DISCORD_TOKEN").expect("Need a DISCORD_TOKEN in env file.");
    let http = Http::new_with_token(&token);
    //code for acquiring list of owners shamelessly taken from serenity repo
    let (owners, bot_id) = match http.get_current_application_info().await {
        Ok(info) => {
            let mut owners = HashSet::new();
            if let Some(team) = info.team {
                owners.insert(team.owner_user_id);
            } else {
                owners.insert(info.owner.id);
            }
            match http.get_current_user().await {
                Ok(bot_id) => (owners, bot_id.id),
                Err(why) => panic!("Could not access the bot id: {:?}", why),
            }
        }
        Err(why) => panic!("Could not access application info: {:?}", why),
    };

    let application_id: u64 = env::var("APPLICATION_ID")
        .expect("Need APPLICATION_ID in env")
        .parse()
        .expect("invalid APPLICATION_ID");
    let framework = StandardFramework::new().configure(|c| {
        c.with_whitespace(true)
            .on_mention(Some(bot_id))
            .prefix("~")
            .owners(owners)
    });
    //.help(&MY_HELP)
    //    .group(&GENERAL_GROUP))

    let mut client = Client::builder(&token)
        .event_handler(Handler)
        .framework(framework)
        .register_songbird()
        .application_id(application_id)
        .intents((GatewayIntents::GUILD_VOICE_STATES | GatewayIntents::GUILD_MEMBERS))
        .await
        .expect("Error creating client");
    {
        let mut data = client.data.write().await;
        data.insert::<CommandCounter>(HashMap::default());
        data.insert::<ShardManagerContainer>(Arc::clone(&client.shard_manager));
    }
    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}
fn check_msg(result: CommandResult<Message>) {
    if let Err(why) = result {
        println!("Error sending message: {:?}", why);
    }
}
