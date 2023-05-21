use bytes_kman::prelude::*;
use libp2p::PeerId;

#[derive(Clone, serde::Serialize, serde::Deserialize, bytes_kman::Bytes)]
pub enum Packet {
    VoicePacket {
        codec: String,
        data: Vec<u8>,
        channel: String,
    },
    VoiceDisconnect {
        channel: String,
    },
    VoiceConnect {
        channel: String,
    },
}
