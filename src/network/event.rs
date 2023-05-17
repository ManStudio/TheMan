use libp2p::PeerId;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum BehaviourEvent {
    VoicePacket {
        from: PeerId,
        codec: String,
        data: Vec<u8>,
    },
    InVoice {
        who: PeerId,
        codec: String,
        chennel: String,
    },
    VoiceRequestConnect {
        from: PeerId,
        codec: String,
        channel: String,
    },
    VoiceAccept {
        to: PeerId,
        codec: String,
        channel: String,
    },
    VoiceConnectedTo {
        to: PeerId,
        codec: String,
        channel: String,
    },
    VoiceDisconnected {
        from: PeerId,
        channel: String,
    },
    VoiceErrorConnection {
        to: PeerId,
        codec: String,
        channel: String,
        error: String,
    },
}
