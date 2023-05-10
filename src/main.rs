use std::sync::{Arc, Mutex};

use gui::TheMan;
use libp2p::identity::Keypair;
use logic::{message::Message, TheManLogic};
use save_state::{Account, TheManSaveState};
use state::TheManState;

pub mod gui;
pub mod logic;
pub mod save_state;
pub mod state;

#[tokio::main]
async fn main() {
    env_logger::init();

    let logic: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>> = Arc::new(Mutex::new(None));
    let lo = logic.clone();

    eframe::run_native(
        "TheMan",
        eframe::NativeOptions {
            always_on_top: false,
            maximized: false,
            decorated: true,
            fullscreen: false,
            drag_and_drop_support: false,
            icon_data: None,
            initial_window_pos: None,
            initial_window_size: None,
            min_window_size: None,
            max_window_size: None,
            resizable: true,
            transparent: false,
            mouse_passthrough: false,
            vsync: true,
            multisampling: 0,
            depth_buffer: 0,
            stencil_buffer: 0,
            hardware_acceleration: eframe::HardwareAcceleration::Preferred,
            renderer: eframe::Renderer::Glow,
            follow_system_theme: true,
            default_theme: eframe::Theme::Dark,
            run_and_return: true,
            event_loop_builder: None,
            shader_version: None,
            centered: true,
        },
        Box::new(|creator| {
            let state: Option<TheManSaveState> =
                if let Some(data) = creator.storage.expect("storage").get_string("state") {
                    if let Ok(state) = ron::from_str(&data) {
                        Some(state)
                    } else {
                        eprintln!("Cannot perse save file");
                        None
                    }
                } else {
                    None
                };
            let state = if let Some(state) = state {
                state
            } else {
                let key_pair = Keypair::generate_ed25519();
                let private = key_pair.to_protobuf_encoding().unwrap();
                TheManSaveState {
                    accounts: vec![Account {
                        name: "Guest".into(),
                        private,
                    }],
                    bootnodes: vec![],
                }
            };

            let mut font_def = eframe::egui::FontDefinitions::empty();
            font_def.font_data.insert(
                "Nerd-Font".into(),
                eframe::egui::FontData::from_static(include_bytes!(
                    "../fonts/Nerd Regular Mono.ttf"
                ))
                .tweak(eframe::egui::FontTweak {
                    scale: 1.0,
                    y_offset_factor: -0.2, // move it up
                    y_offset: 0.0,
                }),
            );

            font_def.families.insert(
                eframe::egui::FontFamily::Monospace,
                vec!["Nerd-Font".to_string()],
            );
            font_def.families.insert(
                eframe::egui::FontFamily::Proportional,
                vec!["Nerd-Font".to_string()],
            );

            creator.egui_ctx.set_fonts(font_def);

            let (gui_sender, logic_reciver) = tokio::sync::mpsc::channel::<Message>(255);
            let (logic_sender, gui_reciver) = tokio::sync::mpsc::channel(255);

            *lo.lock().unwrap() = Some(tokio::spawn(async {
                let state: TheManState = state.into();
                let logic = TheManLogic::new(state, gui_sender, gui_reciver);
                logic.run().await;
            }));

            drop(lo);

            let app = TheMan::new(logic_reciver, logic_sender);
            Box::new(app)
        }),
    )
    .unwrap();

    let logic = Arc::try_unwrap(logic).unwrap().into_inner().unwrap();
    if let Some(logic) = logic {
        logic.await.unwrap()
    }
}
