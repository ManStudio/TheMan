use super::Tab;

#[derive(Default)]
pub struct TabAbout {
    id: usize,
}

impl Tab for TabAbout {
    fn name(&self) -> &str {
        "About"
    }

    fn update(
        &mut self,
        ui: &mut eframe::egui::Ui,
        state: &mut crate::gui::TheManGuiState,
    ) -> Option<String> {
        ui.label("TheMan");
        ui.separator();
        ui.label("Version: 0.0.1");
        ui.label("This is a prototype!");
        ui.label("The hole application GUI is made of tabs you can put a tab any where inside the app you can add a tab by pressing +");
        ui.label("You need to connect to an account for first!");
        ui.label("The account is a random generated key!");
        ui.label("The single thing to create an account is to have the \"Accounts\" tab open and type the name of the account and press add button!");
        ui.label("You need to click an account to connect with it, you will see in the \"Swarm Status\" tab that you will start connecting to other peers!");
        ui.label("The accounts will be saved when the app will be closed");
        ui.label("You can use the \"Friends\" tab to add some one that you know, he will not be notifyed, but if you press Reload button the client will try to connect to it and you will see on his row the Online: true if you are connected to it!");
        ui.label("You can add Message channels and Voice channels in the \"Channels\" tab you can click on any channel to connect!");
        ui.label("In Message Channel you will see any peer that is has that channel in the right!");
        ui.label("Messages will not be saved!");
        ui.label("In Voice Channel you need to connect to any one you want to talk in the right, the other one need to be connected to you to be able to communicate");
        ui.label("Channels will be saved on account");
        ui.separator();
        ui.label("Credits:");
        ui.label("This project is using Opus, Libp2p, egui, eframe, cpal");
        ui.label("The Libp2p is used for the hole network stack!");
        ui.label("Kademila is used for peer discovery");
        ui.label("cpal is used for the audio library");
        ui.label("eframe and egui for the window and the GUI");
        ui.label("Opus is used of encoding and decoding for the audio");
        None
    }

    fn recive(&mut self, message: String) {}

    fn clone_box(&self) -> Box<dyn Tab> {
        Box::<Self>::default()
    }

    fn id(&self) -> usize {
        self.id
    }

    fn set_id(&mut self, id: usize) {
        self.id = id;
    }
}
