use libp2p::Swarm;
use libp2p::core::muxing::StreamMuxerBox;
use libp2p::core::transport::Boxed;
use libp2p::core::identity::Keypair;
use libp2p::gossipsub::{Gossipsub, MessageId};
use libp2p::gossipsub::{
    GossipsubEvent, GossipsubMessage, IdentTopic as Topic, MessageAuthenticity, ValidationMode,
};
use libp2p::{gossipsub, identity, swarm::SwarmEvent, Multiaddr, PeerId};
use std::collections::hash_map::DefaultHasher;
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, Arc};
use std::time::Duration;
use libp2p::futures::StreamExt;


#[derive(Clone)]
pub struct MsgPool {
    swarm: Arc<Mutex<Swarm<Gossipsub>>>
}

impl MsgPool {
    pub fn new(transport :Boxed<(PeerId, StreamMuxerBox)>, local_key: Keypair) -> Self {
        let local_peer_id = PeerId::from(local_key.public());
        let message_id_fn = |message: &GossipsubMessage| {
            let mut s = DefaultHasher::new();
            message.data.hash(&mut s);
            MessageId::from(s.finish().to_string())
        };
        let gossipsub_config = gossipsub::GossipsubConfigBuilder::default()
                .heartbeat_interval(Duration::from_secs(10)) // This is set to aid debugging by not cluttering the log space
                .validation_mode(ValidationMode::Strict) // This sets the kind of message validation. The default is Strict (enforce message signing)
                .message_id_fn(message_id_fn) // content-address messages. No two messages of the
                // same content will be propagated.
                .build()
                .expect("Valid config");
        let mut gossipsub: gossipsub::Gossipsub =
        gossipsub::Gossipsub::new(MessageAuthenticity::Signed(local_key), gossipsub_config)
            .expect("Correct configuration");
        if let Some(explicit) = std::env::args().nth(2) {
                let explicit = explicit.clone();
                match explicit.parse() {
                    Ok(id) => gossipsub.add_explicit_peer(&id),
                    Err(err) => println!("Failed to parse explicit peer id: {:?}", err),
                }
        }
        let topic = Topic::new("test-net");
        gossipsub.subscribe(&topic).unwrap();
        let mut swarm = libp2p::Swarm::new(transport, gossipsub, local_peer_id);
        swarm
            .listen_on("/ip4/0.0.0.0/tcp/0".parse().unwrap())
            .unwrap();
        
        MsgPool{
            swarm: Arc::new(Mutex::new(swarm))
        }
    }

    pub async fn receive_msg_from_network(&mut self) {
        let mut swarm = self.swarm.lock().unwrap();
        let event = swarm.select_next_some();
        match event.await {
            SwarmEvent::Behaviour(GossipsubEvent::Message {
                propagation_source: peer_id,
                message_id: id,
                message,
            }) => println!(
                "Got message: {} with id: {} from peer: {:?}",
                String::from_utf8_lossy(&message.data),
                id,
                peer_id
            ),
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("Listening on {:?}", address);
            },
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                println!("New connection with peer id: {:?}", peer_id);
            }
            _ => {}
        }
    }

    pub fn add_topic_to_swarm(&mut self, topic: Topic) {
        let mut swarm = self.swarm.lock().unwrap();
        let behaviour = swarm.behaviour_mut();
        behaviour.subscribe(&topic).unwrap();
    }

    pub fn add_address_for_swarm(&mut self, address: Multiaddr) {
        let mut swarm = self.swarm.lock().unwrap();
        match swarm.dial(address.clone()) {
            Ok(_) => println!("Dialed {:?}", address),
            Err(e) => println!("Dial {:?} failed: {:?}", address, e),
        };
    }
    
    pub async fn broadcast_msg(&mut self, msg: Vec<u8>, topic: Topic) {
        let mut swarm = self.swarm.lock().unwrap();
        match swarm.behaviour_mut().publish(topic, msg) {
            Ok(msg_id) => println!("broadcast msg successfully: {:?}", msg_id),
            Err(e) => println!("broadcast error: {:?}", e)
        }
    }

}


#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {

    // Create a random PeerId
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("Local peer id: {:?}", local_peer_id);

    // Set up an encrypted TCP Transport over the Mplex and Yamux protocols
    let transport = libp2p::development_transport(local_key.clone()).await?;

    // Create a Gossipsub topic
    let topic = Topic::new("test-net");

    let mut msg_pool = MsgPool::new(transport, local_key);
    
    msg_pool.add_topic_to_swarm(topic.clone());

    if let Some(to_dial) = std::env::args().nth(1) {
        let address: Multiaddr = to_dial.parse().expect("User to provide valid address.");
        msg_pool.add_address_for_swarm(address);
    }

    loop {
        let mut input = local_peer_id.to_bytes();
        input.push(rand::random::<u8>());
        msg_pool.broadcast_msg(input, topic.clone()).await;
        msg_pool.receive_msg_from_network().await;
    }
}