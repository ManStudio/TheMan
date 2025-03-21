use std::{future::Future, pin::Pin};

use eframe::egui;
use iroh::SecretKey;
use iroh_blobs::Hash;
use popup::Popup;
use the_man::base64_deserialize;
use tokio::sync::{mpsc, oneshot, watch};
use tracing::info;

use crate::{Account, Data};

#[derive(Clone, Copy)]
pub enum Event {
    Finished,
}

pub mod pane;
use pane::{Pane, PaneActive, PaneConversations, PaneKnownNodes, PaneOutStreams, PaneStatus};

pub mod popup;

pub mod component;

pub struct Context {
    receiver: mpsc::Receiver<Event>,
    task_sender: mpsc::Sender<Pin<Box<dyn Future<Output = ()> + Send>>>,
    worker: tokio::task::JoinHandle<()>,

    should_save: bool,

    conversation_refreshes: usize,
    tabs: Vec<Box<dyn Pane>>,
    popups: Vec<Box<dyn Popup>>,
}

impl Context {
    pub fn add_task(&self, task: Pin<Box<dyn Future<Output = ()> + Send>>) {
        self.task_sender
            .blocking_send(task)
            .expect("Cannot send task");
    }

    pub fn add_tab(&mut self, pane: impl Pane + 'static) {
        self.tabs.push(Box::new(pane));
    }

    pub fn add_popup(&mut self, popup: impl Popup + 'static) {
        self.popups.push(Box::new(popup));
    }
}

pub struct DashboardManager<'a> {
    destination_tile: egui_tiles::TileId,
    context: &'a mut Context,
    account: &'a mut Account,
    the_man: &'a mut the_man::TheMan,
}

impl egui_tiles::Behavior<Box<dyn Pane>> for DashboardManager<'_> {
    fn pane_ui(
        &mut self,
        ui: &mut eframe::egui::Ui,
        tile_id: egui_tiles::TileId,
        pane: &mut Box<dyn Pane>,
    ) -> egui_tiles::UiResponse {
        pane.ui(ui, self.context, self.the_man, self.account);

        egui_tiles::UiResponse::None
    }

    fn tab_title_for_pane(&mut self, pane: &Box<dyn Pane>) -> eframe::egui::WidgetText {
        pane.name(self.account).into()
    }

    fn simplification_options(&self) -> egui_tiles::SimplificationOptions {
        egui_tiles::SimplificationOptions::OFF
    }

    fn paint_on_top_of_tile(
        &self,
        painter: &egui::Painter,
        _style: &egui::Style,
        tile_id: egui_tiles::TileId,
        rect: egui::Rect,
    ) {
        if self.destination_tile == tile_id {
            painter.rect_stroke(
                rect,
                0.,
                egui::Stroke {
                    width: 1.,
                    color: egui::Color32::DARK_GREEN,
                },
                egui::StrokeKind::Inside,
            );
        }
    }

    fn is_tab_closable(
        &self,
        tiles: &egui_tiles::Tiles<Box<dyn Pane>>,
        tile_id: egui_tiles::TileId,
    ) -> bool {
        let Some(tile) = tiles.get(tile_id) else {
            return false;
        };

        let egui_tiles::Tile::Pane(pane) = tile else {
            return false;
        };

        pane.closable()
    }
}

pub struct Dashboard {
    the_man: the_man::TheMan,
    account_id: usize,
    tree: egui_tiles::Tree<Box<dyn Pane>>,
    popups: Vec<(u32, Box<dyn Popup>)>,
    next_popup: u32,

    destination_tile: egui_tiles::TileId,

    message_receiver: Option<
        oneshot::Receiver<(
            Option<the_man::protocol::Message>,
            watch::Receiver<Option<the_man::protocol::Message>>,
        )>,
    >,
}

impl Dashboard {
    fn new(the_man: the_man::TheMan, data: &mut Data) -> Self {
        let mut tiles = egui_tiles::Tiles::<Box<dyn Pane>>::default();

        let conversations = tiles.insert_pane(Box::new(PaneConversations::default()));
        let known = tiles.insert_pane(Box::new(PaneKnownNodes::default()));
        let active = tiles.insert_pane(Box::new(PaneActive::default()));
        let out_streams = tiles.insert_pane(Box::new(PaneOutStreams::default()));
        let status = tiles.insert_pane(Box::new(PaneStatus::default()));

        let welcome = tiles.insert_pane(Box::new(String::from("Welcome")));

        let left_top = tiles.insert_tab_tile(vec![conversations, known, active, out_streams]);

        let left = tiles.insert_vertical_tile(vec![left_top, status]);
        if let Some(egui_tiles::Tile::Container(egui_tiles::Container::Linear(liniar))) =
            tiles.get_mut(left)
        {
            liniar.shares.set_share(status, 0.05);
        }

        let right = tiles.insert_tab_tile(vec![welcome]);

        let root = tiles.insert_horizontal_tile(vec![left, right]);

        let tree = egui_tiles::Tree::new("tree-dashboard", root, tiles);

        let account_id = data
            .accounts
            .iter()
            .position(|account| account.secret.secret() == the_man.secret().secret())
            .unwrap();

        Self {
            the_man,
            account_id,
            tree,
            destination_tile: right,
            message_receiver: None,
            popups: Default::default(),
            next_popup: 0,
        }
    }

    fn show(&mut self, ui: &mut egui::Ui, context: &mut Context, data: &mut Data) {
        if self.message_receiver.is_none() {
            let message_subscriber = self.the_man.subscribe_messages();
            let (sender, receiver) = oneshot::channel();
            context.add_task(Box::pin(task_message_receiver(message_subscriber, sender)));

            self.message_receiver = Some(receiver);
        }

        if let Some(message_receiver) = &mut self.message_receiver {
            if let Ok((msg, message_subscriber)) = message_receiver.try_recv() {
                context.conversation_refreshes += 1;
                let (sender, receiver) = oneshot::channel();
                context.add_task(Box::pin(task_message_receiver(message_subscriber, sender)));
                self.message_receiver = Some(receiver);
            }
        }

        self.popups.retain_mut(|(id, popup)| {
            let mut close = false;
            let mut res = true;

            egui::Window::new(popup.name())
                .open(&mut res)
                .id(egui::Id::new("POPUP-").with(id))
                .show(ui.ctx(), |ui| {
                    if popup.show(ui, context, &mut data.accounts[self.account_id]) {
                        close = true;
                    }
                });

            if close {
                res = false;
            }

            res
        });

        let mut manager = DashboardManager {
            destination_tile: self.destination_tile,
            context,
            account: &mut data.accounts[self.account_id],
            the_man: &mut self.the_man,
        };

        self.tree.ui(&mut manager, ui);

        for new_tab in std::mem::take(&mut context.tabs) {
            self.add_pane(new_tab);
        }

        for new_popup in std::mem::take(&mut context.popups) {
            self.popups.push((self.next_popup, new_popup));
            self.next_popup += 1;
        }

        if context.should_save {
            context.should_save = false;
            data.save();
        }
    }

    fn add_pane(&mut self, pane: Box<dyn Pane>) {
        let id = self.tree.tiles.insert_pane(pane);
        if let Some(egui_tiles::Tile::Container(container)) =
            self.tree.tiles.get_mut(self.destination_tile)
        {
            container.add_child(id);
        }
    }
}

async fn task_message_receiver(
    mut message_subscriber: watch::Receiver<Option<the_man::protocol::Message>>,
    mut sender: oneshot::Sender<(
        Option<the_man::protocol::Message>,
        watch::Receiver<Option<the_man::protocol::Message>>,
    )>,
) {
    tokio::select! {
        _ = sender.closed() => {

        },
        _ = message_subscriber.changed() => {
            let msg = message_subscriber.borrow().clone();
            _ = sender.send((msg, message_subscriber));
        }
    }
}

pub struct App {
    dashboard: Option<Dashboard>,
    context: Context,
    data: Data,

    create: bool,
    name: String,
    secret: String,

    loading: Option<oneshot::Receiver<the_man::TheMan>>,
}

impl App {
    pub fn new(egui_ctx: &egui::Context, data: Data) -> App {
        let (w_sender, w_receiver) = mpsc::channel::<Pin<Box<dyn Future<Output = ()> + Send>>>(8);
        let (sender, receiver) = mpsc::channel::<Event>(1);

        let egui_ctx = egui_ctx.clone();
        let worker = tokio::spawn(async move {
            let mut w_receiver = w_receiver;
            let sender = sender;

            let mut tasks = Vec::default();

            loop {
                tokio::select! {
                    o_task = w_receiver.recv() => {
                        let Some(task) = o_task else{
                            info!("w_receiver was droped");
                            return;
                        };

                        tasks.push(task);
                    }
                    (_res, index, _rem) = async { if tasks.is_empty() { std::future::pending().await } else {
                        futures_util::future::select_all(&mut tasks).await } } => {

                        _ = tasks.remove(index);

                        if tasks.is_empty(){
                            _ = sender.send(Event::Finished).await;
                        }

                        egui_ctx.request_repaint();
                    }
                };
            }
        });

        Self {
            dashboard: None,
            context: Context {
                receiver,
                task_sender: w_sender,
                worker,

                conversation_refreshes: 0,
                tabs: Default::default(),
                popups: Default::default(),
                should_save: false,
            },
            data,

            create: false,
            name: String::default(),
            secret: String::default(),

            loading: None,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        while let Ok(event) = self.context.receiver.try_recv() {}

        if let Some(dashboard) = &mut self.dashboard {
            egui::CentralPanel::default().show(ctx, |ui| {
                dashboard.show(ui, &mut self.context, &mut self.data)
            });

            return;
        }

        if let Some(mut loading) = self.loading.take() {
            if let Ok(the_man) = loading.try_recv() {
                self.dashboard = Some(Dashboard::new(the_man, &mut self.data));
            } else {
                self.loading = Some(loading);
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.label("Loading The Man");
                });
            }
            return;
        }

        egui::Window::new("Login As").show(ctx, |ui| {
            egui::TopBottomPanel::bottom("login-bottom-panel").show_inside(ui, |ui| {
                if ui
                    .add_sized(ui.available_size(), egui::Button::new("Create"))
                    .clicked()
                {
                    self.create = true;
                }
            });

            for account in self.data.accounts.iter() {
                if ui.button(&account.name).clicked() {
                    let (sender, receiver) = oneshot::channel();
                    let the_man =
                        the_man::TheMan::new(account.name.clone(), account.secret.clone());
                    self.context.add_task(Box::pin(async move {
                        _ = sender.send(the_man.await);
                    }));
                    self.loading = Some(receiver);
                }
            }
        });

        if self.create {
            let mut open = self.create;
            egui::Window::new("Create Account")
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.name).hint_text("Name"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.secret)
                            .password(true)
                            .hint_text("Secret"),
                    );

                    let secret = (!self.secret.is_empty())
                        .then(|| base64_deserialize::<SecretKey>(&self.secret));

                    if let Some(Err(err)) = secret {
                        ui.colored_label(egui::Color32::RED, format!("secret: {err:?}"));
                        return;
                    }

                    ui.separator();

                    if ui.add(egui::Button::new("Add/Create")).clicked() {
                        let secret = secret
                            .map(|res| {
                                res.expect(
                                    "This should be impossibile, if is a error needs to return",
                                )
                            })
                            .unwrap_or_else(|| SecretKey::generate(rand::rngs::OsRng));
                        self.data.accounts.push(crate::Account {
                            name: std::mem::take(&mut self.name),
                            secret,
                            known_as: Default::default(),
                            known_nodes: Default::default(),
                        });
                        self.create = false;
                    }
                });
            self.create &= open;
        }
    }
}

pub trait ToHash {
    fn hash(&self) -> Hash;
}

impl ToHash for iroh::NodeId {
    fn hash(&self) -> Hash {
        Hash::from_bytes(*self.as_bytes())
    }
}

impl ToHash for Hash {
    fn hash(&self) -> Hash {
        *self
    }
}
