use libp2p::{identity::Keypair, PeerId};
use sha3::{Digest, Keccak256};
use std::error::Error;

pub fn parse_peer_id(addr: &String) -> Result<PeerId, Box<dyn Error>> {
  let parts: Vec<&str> = addr.split("/p2p/").collect();
  let str = parts.last().copied().ok_or("Cannot parse peer id.")?;
  let buf = bs58::decode(str).into_vec()?;
  let id = PeerId::from_bytes(&buf)?;
  Ok(id)
}

pub fn ed25519_from_seed(seed: &String) -> Result<Keypair, Box<dyn Error>> {
  let mut hasher = Keccak256::new();
  hasher.update(seed.as_bytes());
  let bytes = hasher.finalize();
  let keypair = Keypair::ed25519_from_bytes(bytes)?;
  Ok(keypair)
}
