use super::{
    message::{AudioMessage, Message},
    TheManLogic,
};

impl TheManLogic {
    pub async fn on_audio_message(&mut self, message: Message) {
        match message {
            Message::Audio(AudioMessage::ResCreateInputChannel(id, error)) => {
                println!("Audio created input: Id: {id}, Error: {error}");
            }
            Message::Audio(AudioMessage::ResCreateOutputChannel(id, error)) => {
                println!("Audio created output: Id: {id}, Error: {error}");
            }
            Message::Audio(AudioMessage::InputData { id, data }) => {
                if let Some(account) = &mut self.state.account {
                    account
                        .swarm
                        .behaviour_mut()
                        .the_man
                        .audio_packet("opus".into(), data)
                }
                // let _ = self
                //     .audio_sender
                //     .try_send(Message::Audio(AudioMessage::OutputData {
                //         id: 1,
                //         data: data,
                //     }));
            }
            _ => {}
        }
    }
}
