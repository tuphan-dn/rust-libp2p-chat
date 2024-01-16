use clap::Parser;
use futures::stream::StreamExt;
use libp2p::{
  autonat, gossipsub, identify,
  identity::Keypair,
  kad::{self, store, BootstrapOk, GetClosestPeersOk, Mode},
  noise, ping,
  swarm::{NetworkBehaviour, SwarmEvent},
  tcp, yamux, SwarmBuilder,
};
use std::{error::Error, time::Duration};
use tokio::{io, io::AsyncBufReadExt, select};
use tracing_subscriber::EnvFilter;

pub mod utils;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
  /// If seed is provided, it will be used to create the keypair.
  #[arg(short, long)]
  seed: Option<String>,
  /// Start with a bootstrap node. If not provided, the current node will become a bootstrap node.
  #[arg(short, long)]
  bootstrap: Option<String>,
  /// Do not print fallback logs.
  #[arg(long, default_value_t = false)]
  silent: bool,
}

#[derive(NetworkBehaviour)]
struct MyBehaviour {
  ping: ping::Behaviour, // To keep the fly alive when connecting
  identify: identify::Behaviour,
  kademlia: kad::Behaviour<store::MemoryStore>,
  autonat: autonat::Behaviour,
  gossipsub: gossipsub::Behaviour,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  use utils::{
    msg::message_id,
    peer::{ed25519_from_seed, parse_peer_id},
  };

  let Args {
    seed,
    bootstrap,
    silent,
  } = Args::parse();
  let port = std::env::var("PORT").unwrap_or("0".to_string());

  // Create a random key for ourselves & read user's inputs
  let keypair = if let Some(s) = seed {
    ed25519_from_seed(&s)?
  } else {
    Keypair::generate_ed25519()
  };

  let _ = tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::from_default_env())
    .try_init();

  let mut swarm = SwarmBuilder::with_existing_identity(keypair.clone())
    .with_tokio()
    .with_tcp(
      tcp::Config::default(),
      noise::Config::new,
      yamux::Config::default,
    )?
    .with_dns()?
    .with_behaviour(|key| {
      // Create a Ping behaviour
      let ping = ping::Behaviour::default();
      // Create a Identify behaviour.
      let identify = identify::Behaviour::new(identify::Config::new(
        "/ipfs/id/1.0.0".to_string(),
        keypair.public(),
      ));
      // Create a Kademlia behaviour.
      let mut cfg = kad::Config::default();
      cfg.set_query_timeout(Duration::from_secs(5 * 60));
      let store = store::MemoryStore::new(key.public().to_peer_id());
      let kademlia = kad::Behaviour::with_config(key.public().to_peer_id(), store, cfg);
      // Create a AutoNAT behaviour.
      let autonat = autonat::Behaviour::new(key.public().to_peer_id(), Default::default());
      // Create a Gossipsub behaviour.
      let gossipsub = gossipsub::Behaviour::new(
        gossipsub::MessageAuthenticity::Signed(key.clone()),
        gossipsub::ConfigBuilder::default()
          .heartbeat_interval(Duration::from_secs(10))
          .validation_mode(gossipsub::ValidationMode::Strict)
          .message_id_fn(message_id)
          .build()?,
      )?;
      // Return my behavour
      Ok(MyBehaviour {
        ping,
        identify,
        kademlia,
        autonat,
        gossipsub,
      })
    })?
    .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(3600))) // Disconnected after 1 hour idle
    .build();

  // Peer node: Listen on all interfaces and whatever port the OS assigns
  swarm.behaviour_mut().kademlia.set_mode(Some(Mode::Server));
  swarm.listen_on(format!("/ip4/0.0.0.0/tcp/{port}").parse()?)?;

  if let Some(bootstrap_addr) = bootstrap {
    // Add peers to the DHT
    swarm
      .behaviour_mut()
      .kademlia
      .add_address(&parse_peer_id(&bootstrap_addr)?, bootstrap_addr.parse()?);
    // Bootstrap the connection
    if let Err(e) = swarm.behaviour_mut().kademlia.bootstrap() {
      println!("⛔️ Failed to run Kademlia bootstrap: {e:?}");
    }
  }

  // Read full lines from stdin
  let mut stdin = io::BufReader::new(io::stdin()).lines();
  let topic = gossipsub::IdentTopic::new("desnet-the-room");
  swarm.behaviour_mut().gossipsub.subscribe(&topic)?;
  println!("💻 Type to send messages to others here:");

  // Kick it off
  loop {
    select! {
      Ok(Some(msg)) = stdin.next_line() => {
        // Publish messages
        if let Err(er)=  swarm.behaviour_mut().gossipsub.publish(topic.clone(), msg.as_bytes()){
          println!("❌ Failed to publish the message: {er}");
        } else {
          println!("🛫 .................. Sent");
        }
      }
      event = swarm.select_next_some() => match event {
        SwarmEvent::NewListenAddr { address, .. } => {
          let mut addr = String::from("");
          addr.push_str(&address.to_string());
          addr.push_str("/p2p/");
          addr.push_str(&keypair.public().to_peer_id().to_string());
          println!("✅ Local node is listening on {addr}");
        }
        SwarmEvent::IncomingConnection { send_back_addr, .. } => {
          println!("⏳ Connecting to {send_back_addr}");
        }
        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
          println!("🔗 Connected to {peer_id}");
        }
        SwarmEvent::ConnectionClosed { peer_id, .. } => {
          println!("💔 Disconnected to {peer_id}");
        }
        // Identify
        SwarmEvent::Behaviour(MyBehaviourEvent::Identify(identify::Event::Received {
          peer_id,
          info,
        })) => {
          println!("👤 Identify new peer: {peer_id}");
          if info.protocols.iter().any(|p| *p == kad::PROTOCOL_NAME) {
            for addr in info.listen_addrs {
              swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
            }
          }
        }
        // Kademlia
        SwarmEvent::Behaviour(MyBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed {
          result: kad::QueryResult::Bootstrap(Ok(BootstrapOk { peer: peer_id, .. })),
          ..
        })) => {
          if peer_id != keypair.public().to_peer_id() {
            println!("🚀 Kademlia bootstrapped completely: {peer_id:?}");
          }
        }
        SwarmEvent::Behaviour(MyBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed {
          result: kad::QueryResult::GetClosestPeers(Ok(GetClosestPeersOk { peers, .. })),
          ..
        })) => {
          println!("🔍 Kademlia discovered new peers: {peers:?}");
        }
        // Gossipsub
        SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Message {
          propagation_source: peer_id,
          message,
          ..
        })) => {
          let msg = String::from_utf8_lossy(&message.data);
          println!("💌 Message from {peer_id}: {msg}");
        }
        // Others
        _ => {
          if silent != true {
            println!("❓ Other Behaviour events {event:?}");
          }
        }
      }
    }
  }
}
