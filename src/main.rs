use async_std::task;
use libp2p::gossipsub::{Gossipsub, MessageId};
use libp2p::gossipsub::{
    GossipsubEvent, GossipsubMessage, IdentTopic as Topic, MessageAuthenticity, ValidationMode,
};
use libp2p::{gossipsub, identity, Multiaddr, PeerId, Swarm};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;
use std::{
    error::Error,
    task::{Context, Poll},
};
use std::sync::{Arc, Mutex};
use libp2p::core::muxing::StreamMuxerBox;
use libp2p::core::transport::Boxed;
use libp2p::futures::{future, StreamExt};
use libp2p::core::identity::Keypair;

#[derive(Clone)]
pub struct MsgPool {
    pub swarm: Arc<Mutex<Swarm<Gossipsub>>>
}

impl MsgPool {
    pub fn new(transport: Boxed<(PeerId, StreamMuxerBox)>, local_key: Keypair) -> Self {
        let local_peer_id = PeerId::from(local_key.public());
        let message_id_fn = |message: &GossipsubMessage| {
            let mut s = DefaultHasher::new();
            message.data.hash(&mut s);
            MessageId::from(s.finish().to_string())
        };

        // Set a custom gossipsub
        let gossipsub_config = gossipsub::GossipsubConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(10)) // This is set to aid debugging by not cluttering the log space
            .validation_mode(ValidationMode::Strict) // This sets the kind of message validation. The default is Strict (enforce message signing)
            .message_id_fn(message_id_fn) // content-address messages. No two messages of the
            // same content will be propagated.
            .build()
            .expect("Valid config");
        // build a gossipsub network behaviour
        let mut gossipsub: gossipsub::Gossipsub =
            gossipsub::Gossipsub::new(MessageAuthenticity::Signed(local_key), gossipsub_config)
                .expect("Correct configuration");

        // add an explicit peer if one was provided
        if let Some(explicit) = std::env::args().nth(2) {
            let explicit = explicit.clone();
            match explicit.parse() {
                Ok(id) => gossipsub.add_explicit_peer(&id),
                Err(err) => println!("Failed to parse explicit peer id: {:?}", err),
            }
        }

        // build the swarm
        let mut swarm = libp2p::Swarm::new(transport, gossipsub, local_peer_id);
        libp2p::Swarm::listen_on(&mut swarm, "/ip4/0.0.0.0/tcp/0".parse().unwrap()).unwrap();
        MsgPool{
            swarm: Arc::new(Mutex::new(swarm))
        }
        // swarm
    }

    fn broadcast_msg(&mut self, msg: Vec<u8>, topic: Topic) {
        let mut swarm = self.swarm.lock().unwrap();
        match swarm.publish(topic, msg) {
            Ok(msg_id) => println!("broadcast msg successfully: {:?}", msg_id),
            Err(e) => println!("broadcast error: {:?}", e)
        }
    }

    fn receive_msg(&mut self, cx: &mut Context<'_>) {
        let mut swarm = self.swarm.lock().unwrap();
        match swarm.poll_next_unpin(cx) {
            Poll::Ready(Some(gossip_event)) => match gossip_event {
                GossipsubEvent::Message {
                    propagation_source: peer_id,
                    message_id: id,
                    message,
                } => println!(
                    "Got message: {} with id: {} from peer: {:?}",
                    String::from_utf8_lossy(&message.data),
                    id,
                    peer_id
                ),
                _ => {}
            },
            Poll::Ready(None) | Poll::Pending => {},
        }
    }

    fn add_address_for_swarm(&mut self, address: String) {
        let mut swarm = self.swarm.lock().unwrap();
        let dialing = address.clone();
        match address.parse() {
            Ok(to_dial) => match libp2p::Swarm::dial_addr(&mut swarm, to_dial) {
                Ok(_) => println!("Dialed {:?}", dialing),
                Err(e) => println!("Dial {:?} failed: {:?}", dialing, e),
            },
            Err(err) => println!("Failed to parse address to dial: {:?}", err),
        }
    }

    fn add_topic_to_swarm(&mut self, topic: Topic) {
        let mut swarm = self.swarm.lock().unwrap();
        swarm.subscribe(&topic).unwrap();
    }

    fn all_listeners(&self) -> Vec<Multiaddr> {
        let swarm= self.swarm.lock().unwrap();
        let listeners: Vec<Multiaddr> = libp2p::Swarm::listeners(&swarm).map(|add| add.clone()).collect();
        listeners
    }

}

fn main() -> Result<(), Box<dyn Error>> {

    // Create a random PeerId
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("Local peer id: {:?}", local_peer_id);

    // Set up an encrypted TCP Transport over the Mplex and Yamux protocols
    let transport = libp2p::build_tcp_ws_noise_mplex_yamux(local_key.clone())?;

    // Create a Gossipsub topic
    let topic = Topic::new("test-net");

    // Create a Swarm to manage peers and events
    let mut msg_pool = MsgPool::new(transport, local_key);
    // let mut swarm = new(transport, local_key);
    msg_pool.add_topic_to_swarm(topic.clone());
    // swarm.subscribe(&topic).unwrap();
    // Reach out to another node if specified
    if let Some(to_dial) = std::env::args().nth(1) {
        msg_pool.add_address_for_swarm(to_dial);
        // add_address_for_swarm(&mut swarm, to_dial);
    }

    let mut listening = false;
    task::block_on(future::poll_fn(move |cx: &mut Context<'_>| {
        // let mut swarm = msg_pool.swarm.lock().unwrap();
        println!("sadadsasadsdaa");
        let listeners = msg_pool.all_listeners();
        if !listeners.is_empty() && !listening {
            for addr in listeners {
                println!("Listening on {:?}", addr);
            }
            listening = true;
        }
        let mut input = local_peer_id.to_bytes();
        let a = rand::random::<u8>();
        println!("random : {}", a);
        input.push(a);

        msg_pool.broadcast_msg( input, topic.clone());
        // broadcast_msg(&mut swarm, input, topic.clone());
        msg_pool.receive_msg(cx);
        // receive_msg(&mut swarm, cx);
        Poll::Pending
    }))
}