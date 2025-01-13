use base64::prelude::*;
use iroh::{protocol::Router, NodeId};
use protocol::{RawConversation, Ticket};
use serde::{de::DeserializeOwned, Serialize};
use tokio::{io::AsyncReadExt, select};
use tracing::info;

pub mod protocol;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("the_man=trace".parse().unwrap()),
        )
        .init();

    let endpoint = iroh::Endpoint::builder()
        .discovery_n0()
        .discovery_dht()
        .bind()
        .await?;

    let gossip = iroh_gossip::net::Gossip::builder()
        .spawn(endpoint.clone())
        .await?;

    let local_pool = iroh_blobs::util::local_pool::LocalPool::default();

    let blobs = iroh_blobs::net_protocol::Blobs::memory().build(local_pool.handle(), &endpoint);

    let the_man = protocol::TheMan::spawn(
        gossip.clone(),
        blobs.clone(),
        endpoint.clone(),
        local_pool.handle(),
    )
    .await;

    info!("Create node");
    let node = iroh::protocol::Router::builder(endpoint)
        .accept(iroh_gossip::ALPN, gossip.clone())
        .accept(iroh_blobs::ALPN, blobs.clone())
        .accept(protocol::ALPN, the_man.clone())
        .spawn()
        .await?;

    println!(
        "NodeId: {}",
        base64_serialize(&node.endpoint().node_id()).unwrap()
    );

    let mut buffer = [0; 1024];
    let mut str = String::default();

    let mut stdin = tokio::io::stdin();
    loop {
        select! {
            Ok(len) = stdin.read(&mut buffer) => {
                if handle_cli(len, &mut str, &mut buffer, &node, &the_man).await {
                    break;
                }
            }
        }
    }

    node.shutdown().await?;

    Ok(())
}

async fn handle_cli(
    len: usize,
    str: &mut String,
    buffer: &mut [u8; 1024],
    node: &Router,
    the_man: &protocol::TheMan<iroh_blobs::store::mem::Store>,
) -> bool {
    str.push_str(&String::from_utf8_lossy(&buffer[0..len]));
    if let Some(i) = str.find('\n') {
        let data = str.drain(0..=i).collect::<Box<str>>();
        let data = data.trim_end();
        let (command, next) = if let Some((command, next)) = data.split_once(' ') {
            (command, Some(next))
        } else {
            (data, None)
        };

        match command {
            "stop" => return true,
            "connect" => {
                let Some(next) = next else {
                    println!("`connect` needs the node hash!");
                    return false;
                };

                let Ok(node_id) = base64_deserialize::<NodeId>(next) else {
                    println!("Cannot parse `node_id`");
                    return false;
                };

                println!("Connecting to: {next}");

                the_man.connect(node_id).await;
            }
            "info" => {
                for info in node.endpoint().remote_info_iter() {
                    println!("{info:#?}");
                }
            }
            "create" => {
                let Some(next) = next else {
                    println!("`create` needs the nodes hashs!");
                    return false;
                };

                let mut nodes = Vec::default();
                for (i, res) in next
                    .split(' ')
                    .map(base64_deserialize::<NodeId>)
                    .enumerate()
                {
                    if let Ok(node_id) = res {
                        nodes.push(node_id);
                    } else {
                        println!("Cannot parse node_id at position: {i}");
                    }
                }

                nodes.push(node.endpoint().node_id());

                let Some(handle) = the_man.create(RawConversation::new(nodes)).await else {
                    eprintln!("Cannot create conversation");
                    return false;
                };

                println!(
                    "Conversation ID: {}",
                    base64_serialize(handle.hash()).unwrap()
                );
            }
            "list" => {
                let Some(next) = next else {
                    println!("Conversations:");
                    for conversation in the_man.conversations().await.unwrap() {
                        let conversation = conversation.get().await;
                        println!(
                            "Ticket: {}",
                            base64_serialize(&conversation.ticket).unwrap()
                        );
                        println!(
                            "\tId: {}",
                            base64_serialize(&conversation.ticket.hash()).unwrap()
                        );
                        println!(
                            "\tNodes: {}",
                            conversation
                                .raw
                                .nodes
                                .iter()
                                .fold(String::default(), |mut acc, peer| {
                                    acc.push_str(&base64_serialize(peer).unwrap());
                                    acc.push(' ');
                                    acc
                                })
                                .trim_end()
                        );
                    }
                    return false;
                };

                let Ok(conversation_id) = base64_deserialize::<iroh_blobs::Hash>(next) else {
                    println!("Cannot parse conversation id");
                    return false;
                };

                let Some(conversation) = the_man.get_conversation(conversation_id).await else {
                    println!("Cannot find conversation");
                    return false;
                };

                println!("Messages:");
                for message in conversation.messages().await {
                    println!("Ticket: {}", base64_serialize(&message.ticket).unwrap());
                    println!(
                        "\tFrom: {}",
                        base64_serialize(&message.ticket.owner_id).unwrap()
                    );
                    println!(
                        "\tTime: {} : {}",
                        message.raw.time.format("%d/%m/%Y %H:%M"),
                        message.raw.data
                    )
                }
            }
            "send" => {
                let Some(next) = next else {
                    println!("`send` needs the conversation and the message");
                    return false;
                };

                let Some((conversation_id, next)) = next.split_once(' ') else {
                    println!("`send` needs the conversation and the message");
                    return false;
                };

                let Ok(conversation_id) = base64_deserialize::<iroh_blobs::Hash>(&conversation_id)
                else {
                    println!("Cannot parse conversation id");
                    return false;
                };

                let Some(conversation) = the_man.get_conversation(conversation_id).await else {
                    println!("Cannot get conversation");
                    return false;
                };

                conversation.send(next).await;
            }
            "recover" => {
                let Some(next) = next else {
                    println!("`reciver` needs a message ticket");
                    return false;
                };

                let Ok(ticket) = base64_deserialize::<Ticket>(next) else {
                    println!("Cannot parse conversation id");
                    return false;
                };

                the_man.recover(ticket).await;
            }
            _ => println!("Invalid command: {command}"),
        }
    }

    false
}

pub fn base64_serialize<T: Serialize>(value: &T) -> bincode::Result<String> {
    let value = bincode::serialize(value)?;
    Ok(BASE64_STANDARD_NO_PAD.encode(value))
}

pub enum Base64DecodeError {
    Bincode(bincode::Error),
    Base64(base64::DecodeError),
}

impl From<base64::DecodeError> for Base64DecodeError {
    fn from(value: base64::DecodeError) -> Self {
        Self::Base64(value)
    }
}

impl From<bincode::Error> for Base64DecodeError {
    fn from(value: bincode::Error) -> Self {
        Self::Bincode(value)
    }
}

pub fn base64_deserialize<T: DeserializeOwned>(
    value: impl AsRef<[u8]>,
) -> Result<T, Base64DecodeError> {
    let bytes = BASE64_STANDARD_NO_PAD.decode(value)?;
    Ok(bincode::deserialize::<T>(&bytes)?)
}
