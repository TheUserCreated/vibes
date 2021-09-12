use std::{
    collections::{HashMap, HashSet},
    env,
    fmt::Write,
    sync::Arc,
};
use serenity::prelude::*;
use serenity::{
    async_trait,
    client::bridge::gateway::{GatewayIntents, ShardId, ShardManager},
    framework::standard::{
        buckets::{LimitedFor, RevertBucket},
        help_commands,
        macros::{check, command, group, help, hook},
        Args,
        CommandGroup,
        CommandOptions,
        CommandResult,
        DispatchError,
        HelpOptions,
        Reason,
        StandardFramework,
    },
    http::Http,
    model::{
        channel::{Channel, Message},
        gateway::Ready,
        id::UserId,
        permissions::Permissions,
    },
    utils::{content_safe, ContentSafeOptions},
};
use tokio::sync::Mutex;
use dotenv;

//Structures needed
//TODO: organize into a different file?
struct ShardManagerContainer;
impl TypeMapKey for ShardManagerContainer {
    type Value = Arc<Mutex<ShardManager>>;
}
struct CommandCounter;

impl TypeMapKey for CommandCounter {
    type Value = HashMap<String, u64>;
}

struct Handler;
#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}



#[group]
#[commands(ping)]
struct General;

async fn my_help(
    context: &Context,
    msg: &Message,
    args: Args,
    help_options: &'static HelpOptions,
    groups: &[&'static CommandGroup],
    owners: HashSet<UserId>,
) -> CommandResult {
    let _ = help_commands::with_embeds(context, msg, args, help_options, groups, owners).await;
    Ok(())
}


#[tokio::main]
async fn main() {
    dotenv::dotenv().expect(("Failed to load env file. Do you have one?"));
    let token = env::var("DISCORD_TOKEN").expect("Need a DISCORD_TOKEN in env file.")
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
        },
        Err(why) => panic!("Could not access application info: {:?}", why),
    };
    let framework = StandardFramework::new()
        .configure(|c| c.with_whitespace(true).on_mention(Some(bot_id))
            .prefix("~")
            .owners(owners)
            .help(&MY_HELP)
            .group(&GENERAL_GROUP));
    let mut client = Client::builder(&token)
        .event_handler(Handler)
        .framework(framework)
        .intents(GatewayIntents::all()).await.expect("Error creating client");
    {
        let mut data = client.data.write().await;
        data.insert::<CommandCounter>(HashMap::default());
        data.insert::<ShardManagerContainer>(Arc::clone(&client.shard_manager));
    }
    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}