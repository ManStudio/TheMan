use super::{
    message::{AudioMessage, Message},
    TheManLogic,
};

impl TheManLogic {
    pub async fn on_audio_message(&mut self, message: Message) {
        match message {
            Message::Audio(AudioMessage::ResCreateInputChannel(id, codec)) => {
                println!("Audio created input: Id: {id}, Codec: {codec}");
            }
            Message::Audio(AudioMessage::ResCreateOutputChannel(id, codec)) => {
                println!("Audio created output: Id: {id}, Codec: {codec}");
            }
            Message::Audio(AudioMessage::InputData { id, data }) => {
                let _ = self
                    .audio_sender
                    .try_send(Message::Audio(AudioMessage::OutputData {
                        id: 1,
                        data: data,
                    }));
            }
            _ => {}
        }
    }
}
