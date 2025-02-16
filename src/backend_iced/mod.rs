use std::{convert::Infallible, sync::Arc};

use iced::{
    advanced::graphics::futures::MaybeSend, widget as W, Element, Length, Renderer, Subscription,
    Task, Theme,
};
use tracing::{error, info};

use crate::Data;

mod screen;

#[derive(Debug, Clone)]
pub enum Screen {
    Login(screen::login::Login),
    Dashboard(screen::dashboard::Dashboard),
}

pub struct TheMan {
    data: Data,
    screen: Screen,
    theme: Theme,
    popups: Vec<Popup<Message>>,
}

impl TheMan {
    pub fn new(data: Data) -> Self {
        Self {
            screen: Screen::Login(screen::login::Login {
                name: String::default(),
                secret: String::default(),
                error: None,
                loggingin: false,
            }),
            theme: Theme::Dark,
            popups: vec![],
            data,
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
    Save,
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
    pub fn update(&mut self, mut message: Message) -> Task<Message> {
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
                    return login.update(&mut self.data, message);
                }
            }
            Message::Dashboard(message) => {
                if let Screen::Dashboard(dashboard) = &mut self.screen {
                    return dashboard.update(message);
                }
            }
            Message::Save => {
                let file =
                    std::fs::File::create("the-man.cbor").expect("Cannot create the-man.cbor");
                ciborium::into_writer(&self.data, file).expect("Cannot save");
                info!("Saved");
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<Message, Theme, Renderer> {
        match &self.screen {
            Screen::Login(login) => login.view(&self.data).map(Message::Login),
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

    pub fn subscription(&self) -> Subscription<Message> {
        match &self.screen {
            Screen::Login(_) => Subscription::none(),
            Screen::Dashboard(dashboard) => dashboard.subscription(),
        }
    }
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
