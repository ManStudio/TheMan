use eframe::egui;

mod about;
mod account;
mod accounts;
mod boot_nodes;
mod channels;
mod discover;
mod friends;
mod message_channel;
mod my_self;
mod peer;
mod peers;
mod query;
mod querys;
mod swarm_status;
mod voice_channel;

pub use about::TabAbout;
pub use account::TabAccount;
pub use accounts::TabAccounts;
pub use boot_nodes::TabBootNodes;
pub use channels::TabChannels;
pub use discover::TabDiscover;
pub use friends::TabFriends;
pub use message_channel::TabMessageChannel;
pub use my_self::TabMySelf;
pub use peer::TabPeer;
pub use peers::TabPeers;
pub use query::TabQuery;
pub use querys::TabQuerys;
pub use swarm_status::TabSwarmStatus;
pub use voice_channel::TabVoiceChannel;

use super::TheManGuiState;

pub trait Tab {
    fn name(&self) -> &str;
    fn update(&mut self, ui: &mut egui::Ui, state: &mut TheManGuiState) -> Option<String>;

    fn recive(&mut self, message: String);

    fn clone_box(&self) -> Box<dyn Tab>;
    fn id(&self) -> usize;
    fn set_id(&mut self, id: usize);
}

pub struct TabManager {
    pub registerd_tabs: Vec<Box<dyn Tab>>,
    pub tabs: egui_dock::Tree<Box<dyn Tab>>,
}

#[allow(clippy::new_without_default)]
impl TabManager {
    pub fn new() -> Self {
        Self {
            registerd_tabs: Vec::new(),
            tabs: egui_dock::Tree::new(Vec::new()),
        }
    }

    pub fn register<T: Tab + 'static + Default>(&mut self) {
        self.registerd_tabs.push(Box::<T>::default());
    }

    /// The script should look like `"o0;o1;o2"`
    /// Every command is separated by `;`
    /// Commands:
    ///     `o(registered_tab: usize, message: String)` = what `registered_tab` to open, `message` what to send
    ///     `f(node: usize)` = what `node` to focus
    ///     `t(node: usize, tab: usize)` = what `tab` to focus in that `node`
    pub fn execute(&mut self, script: &str) {
        let commands = script.split(';').collect::<Vec<&str>>();
        for command in commands {
            let mut chars = command.chars().collect::<Vec<char>>();
            chars.reverse();
            let Some(op) = chars.pop() else {continue};
            match op {
                'o' => {
                    chars.reverse();
                    let string = String::from_iter(chars);
                    let mut values = string.split(',');
                    let error_message = "After t should be tow numbers separate by , like: o0,";
                    let Some(num_str) = values.next() else{eprintln!("{error_message}"); continue};
                    let message = values.collect::<String>();
                    let Ok(num) = num_str.parse::<usize>() else {eprintln!("{error_message}"); continue};

                    if message.is_empty() {
                        self.open(num, None);
                    } else {
                        self.open(num, Some(message));
                    }
                }
                'f' => {
                    chars.reverse();
                    let Ok(num) = String::from_iter(chars).parse::<usize>()else{eprintln!("After o should be a number like: f10"); continue};
                    self.tabs.set_focused_node(num.into());
                }
                't' => {
                    chars.reverse();
                    let string = String::from_iter(chars);
                    let mut values = string.split(',');
                    let message = "After t should be tow numbers separate by , like: t0,1";
                    let Some(node_str) = values.next() else{eprintln!("{message}"); continue};
                    let Some(tab_str) = values.next() else{eprintln!("{message}"); continue};
                    let Ok(node) = node_str.parse::<usize>() else {eprintln!("{message}"); continue};
                    let Ok(tab) = tab_str.parse::<usize>() else {eprintln!("{message}"); continue};
                    self.tabs.set_active_tab(node.into(), tab.into());
                }
                _ => continue,
            }
        }
    }

    pub fn open(&mut self, registered_tab: usize, message: Option<String>) {
        let Some(tab) = self.registerd_tabs.get(registered_tab) else {eprintln!("Invalid registered tab index!"); return};
        let mut used_ids = Vec::new();
        self.tabs
            .iter()
            .flat_map(|node| {
                if let egui_dock::Node::Leaf { tabs, .. } = node {
                    tabs.iter()
                        .filter(|tab2| tab2.name() == tab.name())
                        .collect::<Vec<&Box<dyn Tab>>>()
                } else {
                    vec![]
                }
            })
            .for_each(|tab| used_ids.push(tab.id()));

        let mut id = 1;
        while used_ids.contains(&id) {
            id += 1;
        }

        let mut tab = tab.clone_box();
        tab.set_id(id);
        if let Some(message) = message {
            tab.recive(message)
        }
        self.tabs.push_to_focused_leaf(tab);
    }

    pub fn ui(&mut self, ctx: &egui::Context, state: &mut TheManGuiState) {
        if self.tabs.is_empty() {
            self.open(13, None)
        }

        let mut tab_viewer = TabViewer {
            registered_tabs: &self.registerd_tabs,
            added_tabs: Vec::new(),
            state,
            messages: Vec::new(),
        };

        let mut style = ctx.style().as_ref().clone();
        style.visuals.window_fill = egui::Color32::from_rgb(0x26, 0x26, 0x26);
        style.visuals.panel_fill = egui::Color32::from_rgb(0x27, 0x27, 0x2a);
        style.visuals.hyperlink_color = egui::Color32::from_rgb(0x1e, 0x40, 0xaf);

        ctx.set_style(style);

        let mut style = egui_dock::Style::from_egui(ctx.style().as_ref());
        style.separator.width = 3.0;

        egui_dock::DockArea::new(&mut self.tabs)
            .style(style)
            .show_add_buttons(true)
            .show_add_popup(true)
            .show(ctx, &mut tab_viewer);

        let messages = tab_viewer.messages;
        let added_tabs = tab_viewer.added_tabs;

        for message in messages {
            self.execute(&message);
        }

        for (tab, index) in added_tabs {
            self.tabs.set_focused_node(index);
            self.open(tab, None)
        }
    }
}

pub struct TabViewer<'a> {
    pub registered_tabs: &'a Vec<Box<dyn Tab>>,
    pub added_tabs: Vec<(usize, egui_dock::NodeIndex)>,
    pub state: &'a mut TheManGuiState,
    pub messages: Vec<String>,
}

impl<'a> egui_dock::TabViewer for TabViewer<'a> {
    type Tab = Box<dyn Tab>;

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        if let Some(message) = tab.update(ui, self.state) {
            self.messages.push(message)
        }
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        format!("{} {}", tab.name(), tab.id()).into()
    }

    fn add_popup(&mut self, ui: &mut egui::Ui, node: egui_dock::NodeIndex) {
        ui.style_mut().visuals.button_frame = false;
        ui.set_min_width(100.0);

        for (i, tab) in self.registered_tabs.iter().enumerate() {
            if ui.button(tab.name()).clicked() {
                self.added_tabs.push((i, node));
            }
        }
    }
}
