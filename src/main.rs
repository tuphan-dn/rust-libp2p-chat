use clap::Parser;
use futures::stream::StreamExt;
use libp2p::{
  autonat, identify,
  identity::Keypair,
  kad::{self, store, BootstrapOk, GetClosestPeersOk, Mode},
  noise, relay,
  swarm::{NetworkBehaviour, SwarmEvent},
  tcp, yamux, PeerId, SwarmBuilder,
};
use std::{error::Error, time::Duration};
use tokio::{io, io::AsyncBufReadExt, select};
use tracing_subscriber::EnvFilter;

fn parse_peer_id(addr: &String) -> Result<PeerId, Box<dyn Error>> {
  let parts: Vec<&str> = addr.split("/p2p/").collect();
  let str = parts.last().copied().ok_or("Cannot parse peer id.")?;
  let buf = bs58::decode(str).into_vec()?;
  let id = PeerId::from_bytes(&buf)?;
  Ok(id)
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
  /// Start with a bootstrap node. If not provided, the current node will become a bootstrap node.
  #[arg(short, long)]
  bootstrap: Option<String>,
  /// Do not print fallback logs.
  #[arg(short, long, default_value_t = false)]
  silent: bool,
}

#[derive(NetworkBehaviour)]
struct MyBehaviour {
  identify: identify::Behaviour,
  kademlia: kad::Behaviour<store::MemoryStore>,
  autonat: autonat::Behaviour,
  relay: relay::Behaviour,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  // Create a random key for ourselves & read user's inputs
  let keypair = Keypair::generate_ed25519();
  let Args { bootstrap, silent } = Args::parse();

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
      // Create a Relay behaviour.
      let relay = relay::Behaviour::new(key.public().to_peer_id(), Default::default());
      // Return my behavour
      Ok(MyBehaviour {
        identify,
        kademlia,
        autonat,
        relay,
      })
    })?
    .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(3600))) // Disconnected after 1 hour idle
    .build();

  // Peer node: Listen on all interfaces and whatever port the OS assigns
  swarm.behaviour_mut().kademlia.set_mode(Some(Mode::Server));
  swarm.listen_on("/ip4/0.0.0.0/tcp/8080".parse()?)?;

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
  println!("💻 Enter a multiaddr to the closest peer:");

  // Kick it off
  loop {
    select! {
      Ok(Some(multiaddr)) = stdin.next_line() => {
        // Search for the closest peers
        println!("🔍 Searching for the closest peer to {multiaddr}");
        swarm.behaviour_mut().kademlia.get_closest_peers(parse_peer_id(&multiaddr)?);
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
