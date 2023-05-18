use libp2p::PeerId;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub enum Packet {
    VoicePacket {
        codec: String,
        data: Vec<u8>,
        channel: String,
    },
    VoiceRequest {
        codec: String,
        channel: String,
    },
    VoiceAccept {
        codec: String,
        channel: String,
    },
    VoiceRefuze {
        codec: String,
        channel: String,
    },
    VoiceDisconnect {
        channel: String,
    },
}
