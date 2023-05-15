use super::{
    message::{AudioMessage, Message},
    TheManLogic,
};

impl TheManLogic {
    pub async fn on_audio_message(&mut self, message: Message) {
        match message {
            Message::Audio(AudioMessage::ResCreateInputChannel(id, error)) => {
                println!("Audio created input: Id: {id}, Error: {codec}");
            }
            Message::Audio(AudioMessage::ResCreateOutputChannel(id, error)) => {
                println!("Audio created output: Id: {id}, Error: {codec}");
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
