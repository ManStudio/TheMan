use std::sync::{Arc, Mutex};

use audio::Audio;
use chrono::Utc;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use gui::TheMan;
use libp2p::identity::Keypair;
use logic::{message::Message, TheManLogic};
use save_state::{Account, TheManSaveState};
use state::TheManState;

pub mod audio;
pub mod gui;
pub mod logic;
pub mod save_state;
pub mod state;

#[tokio::main]
async fn main() {
    env_logger::init();
    {
        let cpal = cpal::default_host();

        let device = cpal.default_output_device().expect("Output device");
        let config = device.default_output_config().expect("Output config");

        let sample_rate = config.sample_rate().0 as f32;
        let channels = config.channels() as usize;

        let mut sample_clock = 0.0;
        let mut next_value = move || {
            sample_clock = (sample_clock + 1.0) % sample_rate;
            (sample_clock * 440.0 * 2.0 * std::f32::consts::PI / sample_rate).sin()
        };

        std::thread::spawn(move || {
            let stream = device
                .build_output_stream(
                    &config.into(),
                    move |data: &mut [f32], _| {
                        for frame in data.chunks_mut(channels) {
                            let value: f32 = next_value();
                            for sample in frame.iter_mut() {
                                *sample = value;
                            }
                        }
                    },
                    |_| (),
                    None,
                )
                .unwrap();
            let _ = stream.play();
            std::thread::sleep(std::time::Duration::from_millis(100));
        });
    }

    let logic: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>> = Arc::new(Mutex::new(None));
    let lo = logic.clone();
    let audio: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>> = Arc::new(Mutex::new(None));
    let au = audio.clone();

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
            multisampling: 8,
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
            active: true,
            app_id: Some("theman".to_string()),
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
                        friends: vec![],
                        expires: Utc::now(),
                        channels: vec![],
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
                    y_offset_factor: -0.2,
                    y_offset: 0.0,
                    baseline_offset_factor: 0.0,
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
            let egui_ctx = creator.egui_ctx.clone();

            use tokio::sync::mpsc::channel;
            let (gui_logic_sender, gui_logic_receiver) = channel(255);
            let (logic_gui_sender, logic_gui_receiver) = channel(255);

            let (logic_audio_sender, logic_audio_receiver) = channel(255);
            let (audio_logic_sender, audio_logic_receiver) = channel(255);

            *au.lock().unwrap() = Some(tokio::spawn(async {
                let audio = Audio::new(logic_audio_sender, audio_logic_receiver);
                audio.run().await;
            }));

            drop(au);

            *lo.lock().unwrap() = Some(tokio::spawn(async {
                let state: TheManState = state.into();
                let logic = TheManLogic::new(
                    state,
                    gui_logic_sender,
                    logic_gui_receiver,
                    egui_ctx,
                    audio_logic_sender,
                    logic_audio_receiver,
                );
                logic.run().await;
            }));

            drop(lo);

            let app = TheMan::new(gui_logic_receiver, logic_gui_sender);
            Box::new(app)
        }),
    )
    .unwrap();

    let audio = Arc::try_unwrap(audio).unwrap().into_inner().unwrap();
    if let Some(audio) = audio {
        audio.await.unwrap()
    }
    let logic = Arc::try_unwrap(logic).unwrap().into_inner().unwrap();
    if let Some(logic) = logic {
        logic.await.unwrap()
    }
}
