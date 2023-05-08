use std::sync::{Arc, Mutex};

use gui::TheMan;
use libp2p::identity::Keypair;
use logic::{message::Message, TheManLogic};
use save_state::TheManSaveState;
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
            let state: TheManSaveState =  if let Some(data) = creator.storage.expect("storage").get_string("state") {
                ron::from_str(&data).unwrap()
            }else{
                let key_pair = Keypair::generate_ed25519();
                let private = key_pair.to_protobuf_encoding().unwrap();
                TheManSaveState { private, nodes: vec![
        		"/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN".parse().unwrap(),
        		"/dnsaddr/bootstrap.libp2p.io/p2p/QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa".parse().unwrap(),
        		"/dnsaddr/bootstrap.libp2p.io/p2p/QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb".parse().unwrap(),
        		"/dnsaddr/bootstrap.libp2p.io/p2p/QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt".parse().unwrap(),
        		"/ip4/104.131.131.82/tcp/4001/p2p/QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ".parse().unwrap(),
        		"/ip4/104.131.131.82/udp/4001/quic/p2p/QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ".parse().unwrap()] }
            };


            let (gui_sender, logic_reciver) = tokio::sync::mpsc::channel::<Message>(255);
            let (logic_sender, gui_reciver) = tokio::sync::mpsc::channel(255);

            *lo.lock().unwrap() = Some(tokio::spawn(async{
                let state:TheManState = state.into();
                let logic = TheManLogic::new(state, gui_sender, gui_reciver);
                logic.run().await;
                }
            ));

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
