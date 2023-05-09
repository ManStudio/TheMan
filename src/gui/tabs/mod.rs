use eframe::egui;

mod boot_nodes;
mod peers;
mod swarm_status;

pub use boot_nodes::TabBootNodes;
pub use peers::TabPeers;
pub use swarm_status::TabSwarmStatus;

use super::TheManGuiState;

pub trait Tab {
    fn name(&self) -> &str;
    fn update(&mut self, ui: &mut egui::Ui, state: &mut TheManGuiState);

    fn clone_box(&self) -> Box<dyn Tab>;
    fn id(&self) -> usize;
    fn set_id(&mut self, id: usize);
}

pub struct TabManager {
    pub registerd_tabs: Vec<Box<dyn Tab>>,
    pub tabs: egui_dock::Tree<Box<dyn Tab>>,
}

impl TabManager {
    pub fn new() -> Self {
        Self {
            registerd_tabs: Vec::new(),
            tabs: egui_dock::Tree::new(Vec::new()),
        }
    }

    pub fn register<T: Tab + 'static + Default>(&mut self) {
        self.registerd_tabs.push(Box::new(T::default()));
    }

    /// The script should look like `"o0;o1;o2"`
    /// Every command is separated by `;`
    /// Commands:
    ///     `o(registered_tab: usize)` = what `registered_tab` to open
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
                    let Ok(num) = String::from_iter(chars).parse::<usize>()else{eprintln!("After o should be a number like: o10"); continue};
                    self.open(num);
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

    pub fn open(&mut self, registered_tab: usize) {
        let Some(tab) = self.registerd_tabs.get(registered_tab) else {eprintln!("Invalid registered tab index!"); return};
        let mut tab = tab.clone_box();
        let mut used_ids = Vec::new();
        let test = self
            .tabs
            .iter()
            .map(|node| {
                if let egui_dock::Node::Leaf { tabs, .. } = node {
                    tabs.iter()
                        .filter(|tab2| tab2.name() == tab.name())
                        .collect::<Vec<&Box<dyn Tab>>>()
                } else {
                    vec![]
                }
            })
            .flatten()
            .for_each(|tab| used_ids.push(tab.id()));

        let mut id = 1;
        while used_ids.contains(&id) {
            id += 1;
        }

        let mut tab = tab.clone_box();
        tab.set_id(id);
        self.tabs.push_to_focused_leaf(tab);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, state: &mut TheManGuiState) {
        let mut tab_viewer = TabViewer {
            registered_tabs: &self.registerd_tabs,
            added_tabs: Vec::new(),
            state,
        };

        egui_dock::DockArea::new(&mut self.tabs)
            .show_add_buttons(true)
            .show_add_popup(true)
            .show_inside(ui, &mut tab_viewer);

        tab_viewer.added_tabs.drain(..).for_each(|(tab, index)| {
            self.tabs.set_focused_node(index);
            self.open(tab)
        });
    }
}

pub struct TabViewer<'a> {
    pub registered_tabs: &'a Vec<Box<dyn Tab>>,
    pub added_tabs: Vec<(usize, egui_dock::NodeIndex)>,
    pub state: &'a mut TheManGuiState,
}

impl<'a> egui_dock::TabViewer for TabViewer<'a> {
    type Tab = Box<dyn Tab>;

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        tab.update(ui, self.state)
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
