use iced::Task;
use iced::{widget as W, Element, Renderer, Theme};
use iroh::SecretKey;
use the_man::{base64_deserialize, base64_serialize, TheMan};
use tracing::info;

use crate::{protocol, screen};
use crate::{Message as TMessage, Screen};

#[derive(Clone, Debug)]
pub enum Message {
    Login(String, SecretKey),
    SetName(String),
    SetSecret(String),
    SubmitName,
    SubmitSecret,
}

#[derive(Debug, Clone)]
pub struct Login {
    pub name: String,
    pub secret: String,
    pub error: Option<String>,
    pub accounts: Vec<(String, SecretKey)>,
    pub loggingin: bool,
}

impl Login {
    pub fn update(&mut self, message: Message) -> Task<TMessage> {
        match message {
            Message::Login(name, secret_key) => {
                self.loggingin = true;
                return Task::perform(
                    async move {
                        let the_man = TheMan::new(secret_key).await;

                        let message_receiver = the_man.subscribe_messages();

                        Screen::Dashboard(screen::dashboard::Dashboard::new(
                            name,
                            the_man,
                            message_receiver,
                        ))
                    },
                    TMessage::ChangeScreen,
                );
            }
            Message::SetName(name) => self.name = name,
            Message::SetSecret(secret) => self.secret = secret,
            Message::SubmitName => return W::focus_next(),
            Message::SubmitSecret => {
                let Some(secret) = self
                    .secret
                    .is_empty()
                    .then(|| SecretKey::generate(rand::rngs::OsRng))
                    .or_else(|| {
                        base64_deserialize::<SecretKey>(&self.secret)
                            .inspect_err(|err| {
                                self.error = Some(format!("Invalid secret: {err:?}"));
                            })
                            .ok()
                    })
                else {
                    return Task::none();
                };

                self.accounts.push((std::mem::take(&mut self.name), secret));

                self.secret.clear();
            }
        }

        Task::none()
    }

    pub fn view(&self) -> Element<Message> {
        if self.loggingin {
            return W::center(W::text("Logging in...")).into();
        }

        let mut elements = Vec::<Element<'_, Message, Theme, Renderer>>::new();
        for (name, secret) in self.accounts.iter() {
            elements.push(
                W::button(W::row![
                    W::horizontal_space(),
                    W::text(name.clone()),
                    W::horizontal_space(),
                    W::text(base64_serialize(&secret.public()).unwrap()),
                    W::horizontal_space()
                ])
                .on_press(Message::Login(name.clone(), secret.clone()))
                .into(),
            );
        }

        let error: Element<'_, Message, Theme, Renderer> = self
            .error
            .as_ref()
            .map(|error| W::text(error).color(iced::color!(0xff0000)).into())
            .unwrap_or_else(|| W::row![].into());

        Element::from(W::column![
            W::container(W::text("Login as:").size(21)).padding(5),
            W::container(W::column(elements)).padding(10),
            W::vertical_space(),
            error,
            W::container(W::row![
                W::text_input("Name", &self.name)
                    .on_input(Message::SetName)
                    .on_submit(Message::SubmitName),
                W::text_input("Secret", &self.secret)
                    .on_input(Message::SetSecret)
                    .on_submit(Message::SubmitSecret),
                W::button("Add").on_press(Message::SubmitSecret)
            ])
            .padding(5)
        ])
    }
}
