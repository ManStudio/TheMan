use std::{collections::VecDeque, convert::Infallible, sync::Arc, time::Duration};

use base64::prelude::*;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use iced::{
    advanced::graphics::futures::MaybeSend, widget as W, Element, Length, Renderer, Subscription,
    Task, Theme,
};
use iroh::{protocol::Router, NodeId};
use serde::{de::DeserializeOwned, Serialize};
use the_man::{
    base64_deserialize, base64_serialize,
    protocol::{self, RawConversation, Ticket},
};
use tracing::{error, info, instrument::WithSubscriber};

mod screen;

#[derive(Debug, Clone)]
pub enum Screen {
    Login(screen::login::Login),
    Dashboard(screen::dashboard::Dashboard),
}

pub struct TheMan {
    screen: Screen,
    theme: Theme,
    popups: Vec<Popup<Message>>,
}

impl TheMan {
    pub fn new() -> Self {
        Self {
            screen: Screen::Login(screen::login::Login {
                name: String::default(),
                secret: String::default(),
                error: None,
                accounts: Vec::default(),
                loggingin: false,
            }),
            theme: Theme::Dark,
            popups: vec![],
        }
    }
}

#[derive(Clone)]
pub enum Message {
    CreatePopup(Popup<Message>),
    ClosePopup,
    ChangeScreen(Screen),
    Login(screen::login::Message),
    Dashboard(screen::dashboard::Message),
}

impl From<Message> for screen::dashboard::Message {
    fn from(value: Message) -> Self {
        if let Message::Dashboard(value) = value {
            value
        } else {
            panic!()
        }
    }
}

impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Message").finish()
    }
}

impl TheMan {
    fn update(&mut self, mut message: Message) -> Task<Message> {
        if let Message::ClosePopup = &message {
            let Some(popup) = self.popups.pop() else {
                error!("No popup to close!");
                return Task::none();
            };
            let Some(n_message) = popup.0.finish() else {
                return Task::none();
            };
            message = n_message;
        }

        if let Some(popup) = self.popups.last_mut() {
            let task = popup.update(message);
            if let Some(m) = popup.finish() {
                self.popups.pop();
                message = m;
            } else {
                return task;
            }
        }

        match message {
            Message::CreatePopup(popup) => {
                self.popups.push(popup);
            }
            Message::ClosePopup => unreachable!(),
            Message::ChangeScreen(screen) => {
                self.screen = screen;
            }
            Message::Login(message) => {
                if let Screen::Login(login) = &mut self.screen {
                    return login.update(message);
                }
            }
            Message::Dashboard(message) => {
                if let Screen::Dashboard(dashboard) = &mut self.screen {
                    return dashboard.update(message);
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<Message, Theme, Renderer> {
        match &self.screen {
            Screen::Login(login) => login.view().map(Message::Login),
            Screen::Dashboard(dashboard) => {
                let body = dashboard.view().map(Message::Dashboard);
                if let Some(popup) = self.popups.last() {
                    let popup = W::center(W::column![
                        W::container(W::row![
                            W::horizontal_space().width(iced::Length::Fill),
                            W::button("X").on_press(Message::ClosePopup)
                        ])
                        .style(|_| W::container::background(iced::color!(0x212121))),
                        W::container(popup.view()).height(Length::Fill)
                    ])
                    .padding(20)
                    .style(W::container::rounded_box);
                    popup.into()
                } else {
                    body
                }
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        match &self.screen {
            Screen::Login(_) => Subscription::none(),
            Screen::Dashboard(dashboard) => dashboard.subscription(),
        }
    }
}

const CHANNELS: usize = 1;
const SAMPLE_RATE: usize = 48000;
const BITRATE: usize = 64000;
const FRAME_SIZE: usize = SAMPLE_RATE / (1000 / 5); // MS

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("the_man=trace".parse().unwrap()),
        )
        .init();

    // audio_testing_opus();

    // return Ok(());

    let app = iced::application("TheMan", TheMan::update, TheMan::view)
        .subscription(TheMan::subscription);
    app.run_with(|| (TheMan::new(), Task::none())).unwrap();

    return Ok(());

    // let endpoint = iroh::Endpoint::builder()
    //     .discovery_n0()
    //     .discovery_dht()
    //     .bind()
    //     .await?;

    // let gossip = iroh_gossip::net::Gossip::builder()
    //     .spawn(endpoint.clone())
    //     .await?;

    // let local_pool = iroh_blobs::util::local_pool::LocalPool::default();

    // let blobs = iroh_blobs::net_protocol::Blobs::memory().build(local_pool.handle(), &endpoint);

    // let the_man = protocol::TheMan::spawn(
    //     gossip.clone(),
    //     blobs.clone(),
    //     endpoint.clone(),
    //     local_pool.handle(),
    // )
    // .await;

    // info!("Create node");
    // let node = iroh::protocol::Router::builder(endpoint)
    //     .accept(iroh_gossip::ALPN, gossip.clone())
    //     .accept(iroh_blobs::ALPN, blobs.clone())
    //     .accept(protocol::ALPN, the_man.clone())
    //     .spawn()
    //     .await?;

    // println!(
    //     "NodeId: {}",
    //     base64_serialize(&node.endpoint().node_id()).unwrap()
    // );

    // let mut buffer = [0; 1024];
    // let mut str = String::default();

    // let mut stdin = tokio::io::stdin();
    // loop {
    //     select! {
    //         Ok(len) = stdin.read(&mut buffer) => {
    //             if handle_cli(len, &mut str, &mut buffer, &node, &the_man).await {
    //                 break;
    //             }
    //         }
    //     }
    // }

    // node.shutdown().await?;

    // Ok(())
}

pub trait DynClone {
    fn clone_box(&self) -> *mut Infallible;
}

impl<T: Clone> DynClone for T {
    fn clone_box(&self) -> *mut Infallible {
        Box::into_raw(Box::new(self.clone())) as *mut _
    }
}

pub trait TPopup<Message>: Send + Sync + DynClone {
    fn update(&mut self, message: Message) -> Task<Message>;
    fn view(&self) -> Element<Message, Theme, Renderer>;
    fn finish(&self) -> Option<Message>;
}

impl<Message> TPopup<Message> for Infallible {
    fn update(&mut self, _message: Message) -> Task<Message> {
        unreachable!()
    }

    fn view(&self) -> Element<Message, Theme, Renderer> {
        unreachable!()
    }

    fn finish(&self) -> Option<Message> {
        unreachable!()
    }
}

impl<M> Clone for Box<dyn TPopup<M>> {
    fn clone(&self) -> Self {
        unsafe { Box::from_raw(self.clone_box() as *mut _) }
    }
}

#[derive(Clone)]
pub struct Popup<M>(Box<dyn TPopup<M> + 'static>);

impl<M: Clone + MaybeSend + Sync + 'static> Popup<M> {
    pub fn new(popup: impl TPopup<M> + 'static) -> Self {
        Self(Box::new(popup))
    }

    pub fn map<T: Clone + MaybeSend + Sync + 'static>(
        self,
        from_to: impl MapFunction<M, T> + 'static,
        to_from: impl MapFunction<T, M> + 'static,
    ) -> Popup<T> {
        Popup(Box::new(MapPopup {
            popup: self,
            map_from_to: Arc::new(from_to),
            map_to_from: Arc::new(to_from),
        }))
    }
}

impl<M: Clone> TPopup<M> for Popup<M> {
    fn update(&mut self, message: M) -> Task<M> {
        self.0.update(message)
    }

    fn view(&self) -> Element<M, Theme, Renderer> {
        self.0.view()
    }

    fn finish(&self) -> Option<M> {
        self.0.finish()
    }
}

pub trait MapFunction<FROM: MaybeSend + Sync, TO: MaybeSend + Sync>:
    Fn(FROM) -> TO + MaybeSend + Sync
{
}

impl<FROM, TO, T: Fn(FROM) -> TO> MapFunction<FROM, TO> for T
where
    FROM: MaybeSend + Sync + 'static,
    TO: MaybeSend + Sync + 'static,
    T: MaybeSend + Sync + 'static,
{
}

#[derive(Clone)]
pub struct MapPopup<
    TO: MaybeSend + Sync + Clone + 'static,
    FROM: MaybeSend + Sync + Clone + 'static,
> {
    popup: Popup<FROM>,
    map_from_to: Arc<dyn MapFunction<FROM, TO>>,
    map_to_from: Arc<dyn MapFunction<TO, FROM>>,
}

impl<TO: MaybeSend + Sync + Clone + 'static, FROM: MaybeSend + Sync + Clone + 'static> TPopup<TO>
    for MapPopup<TO, FROM>
{
    fn update(&mut self, message: TO) -> Task<TO> {
        let f = (self.map_to_from)(message);
        let map_from_to = self.map_from_to.clone();
        self.popup.update(f).map(move |m| (map_from_to)(m))
    }

    fn view(&self) -> Element<TO, Theme, Renderer> {
        self.popup.view().map(|m| (self.map_from_to)(m))
    }

    fn finish(&self) -> Option<TO> {
        self.popup.0.finish().map(|m| (self.map_from_to)(m))
    }
}

fn setup_audio() -> (
    cpal::Stream,
    cpal::Stream,
    std::sync::mpsc::Receiver<f32>,
    std::sync::mpsc::Sender<f32>,
) {
    let host = cpal::default_host();
    dbg!(host.id());

    let input_device = host.default_input_device().expect("Input device");
    info!("Input Device: {:?}", input_device.name().unwrap());

    let (input_sender, input_receiver) = std::sync::mpsc::channel::<f32>();

    let input_stream = input_device
        .build_input_stream(
            &cpal::StreamConfig {
                channels: CHANNELS as u16,
                sample_rate: cpal::SampleRate(48000),
                buffer_size: cpal::BufferSize::Fixed(960),
            },
            move |data: &[f32], info| {
                for sample in data {
                    input_sender.send(*sample).expect("Cannot send");
                }
            },
            |err| error!("Input {err}"),
            None,
        )
        .unwrap();

    let output_device = host.default_output_device().expect("Output device");
    info!("Output Device: {:?}", output_device.name());

    let output_config = output_device.default_output_config().unwrap();
    dbg!(output_config);
    let mut buffer = VecDeque::default();
    let mut ii = 0.05;
    for i in 0..48000 * 4 {
        buffer.push_back((i as f32 * ii).sin() * 0.1);
        if i % (48000 / 8) == 0 {
            ii += 0.01;
        }
    }

    let (output_sender, output_receiver) = std::sync::mpsc::channel::<f32>();

    let output_stream = output_device
        .build_output_stream(
            &cpal::StreamConfig {
                channels: CHANNELS as u16,
                sample_rate: cpal::SampleRate(48000),
                buffer_size: cpal::BufferSize::Fixed(960),
            },
            move |data: &mut [f32], info| {
                for sample in data {
                    *sample = output_receiver.try_recv().unwrap_or(0.0);
                }
            },
            |err| error!("Output: {err}"),
            None,
        )
        .unwrap();

    (input_stream, output_stream, input_receiver, output_sender)
}

pub struct ViewSensor<Message: Clone> {
    on_in_view: Message,
}

impl<Renderer: iced::advanced::Renderer, Theme, Message: Clone>
    iced::advanced::Widget<Message, Theme, Renderer> for ViewSensor<Message>
{
    fn size(&self) -> iced::Size<Length> {
        iced::Size {
            width: 0.into(),
            height: 0.into(),
        }
    }

    fn layout(
        &self,
        _tree: &mut iced::advanced::widget::Tree,
        _renderer: &Renderer,
        _limits: &iced::advanced::layout::Limits,
    ) -> iced::advanced::layout::Node {
        iced::advanced::layout::Node::new(iced::Size {
            width: 0.0,
            height: 0.0,
        })
    }

    fn draw(
        &self,
        _tree: &iced::advanced::widget::Tree,
        _renderer: &mut Renderer,
        _theme: &Theme,
        _style: &iced::advanced::renderer::Style,
        _layout: iced::advanced::Layout<'_>,
        _cursor: iced::advanced::mouse::Cursor,
        _viewport: &iced::Rectangle,
    ) {
    }

    fn on_event(
        &mut self,
        _state: &mut iced::advanced::widget::Tree,
        event: iced::Event,
        layout: iced::advanced::Layout<'_>,
        _cursor: iced::advanced::mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn iced::advanced::Clipboard,
        shell: &mut iced::advanced::Shell<'_, Message>,
        viewport: &iced::Rectangle,
    ) -> iced::advanced::graphics::core::event::Status {
        if layout.bounds().is_within(viewport) {
            if let iced::Event::Window(iced::window::Event::RedrawRequested(_)) = &event {
                shell.publish(self.on_in_view.clone());
            }
        }
        iced::advanced::graphics::core::event::Status::Ignored
    }
}
