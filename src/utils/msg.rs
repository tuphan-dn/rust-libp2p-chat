use libp2p::gossipsub::{Message, MessageId};
use std::{
  collections::hash_map::DefaultHasher,
  hash::{Hash, Hasher},
};

pub fn message_id(message: &Message) -> MessageId {
  let mut s = DefaultHasher::new();
  message.data.hash(&mut s);
  MessageId::from(s.finish().to_string())
}
