use libp2p::PeerId;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum BehaviourEvent {
    VoicePacket {
        from: PeerId,
        codec: String,
        data: Vec<u8>,
        channel: String,
    },
    Request {
        channel: String,
        from: PeerId,
    },
    Disconnected {
        channel: String,
        from: PeerId,
    },
    VoiceDisconnected {
        from: PeerId,
    },
    VoiceErrorConnection {
        to: PeerId,
        codec: String,
        channel: String,
        error: String,
    },
}
