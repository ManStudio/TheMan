use std::sync::{Arc, Mutex};

use audio::Audio;
use chrono::Utc;
use glow::HasContext;
use glutin::{
    config::ConfigTemplateBuilder,
    context::ContextAttributesBuilder,
    display::GetGlDisplay,
    prelude::{GlDisplay, NotCurrentGlContextSurfaceAccessor},
    surface::{GlSurface, SurfaceAttributesBuilder, WindowSurface},
};
use glutin_winit::{DisplayBuilder, GlWindow};
use gui::TheMan;
use libp2p::identity::Keypair;
use logic::{message::Message, TheManLogic};
use raw_window_handle::HasRawWindowHandle;
use save_state::{Account, TheManSaveState};
use state::TheManState;
use winit::{
    dpi::PhysicalSize, platform::run_return::EventLoopExtRunReturn, window::WindowBuilder,
};

pub mod audio;
pub mod gui;
pub mod logic;
pub mod save_state;
pub mod state;

#[tokio::main]
async fn main() {
    env_logger::init();
    let logic: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>> = Arc::new(Mutex::new(None));
    let lo = logic.clone();
    let audio: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>> = Arc::new(Mutex::new(None));
    let au = audio.clone();

    let mut event_loop: winit::event_loop::EventLoop<Message> =
        winit::event_loop::EventLoopBuilder::with_user_event().build();
    let (window, config) = DisplayBuilder::new()
        .with_window_builder(Some(WindowBuilder::new().with_title("TheMan")))
        .build(&event_loop, ConfigTemplateBuilder::new(), |mut configs| {
            configs.next().unwrap()
        })
        .unwrap();

    let window = window.unwrap();
    let display = config.display();
    let context = unsafe {
        display
            .create_context(
                &config,
                &ContextAttributesBuilder::new().build(Some(window.raw_window_handle())),
            )
            .unwrap()
    };

    let window_attribs =
        window.build_surface_attributes(SurfaceAttributesBuilder::<WindowSurface>::new());

    let surface = unsafe {
        display
            .create_window_surface(&config, &window_attribs)
            .unwrap()
    };

    let context = context.make_current(&surface).unwrap();

    let gl = unsafe {
        glow::Context::from_loader_function_cstr(|symbol| display.get_proc_address(symbol))
    };

    let gl = Arc::new(gl);

    let egui_context = egui::Context::default();
    let mut egui_state = egui_winit::State::new(&event_loop);
    let mut egui_painter = egui_glow::Painter::new(gl.clone(), "", None).unwrap();

    let save_dir = dirs::data_local_dir().unwrap().join("theman");

    let state: Option<TheManSaveState> =
        if let Ok(data) = std::fs::read_to_string(save_dir.join("app.ron")) {
            match ron::from_str(&data) {
                Ok(state) => Some(state),
                Err(error) => {
                    eprintln!("Cannot perse save file: {error}");
                    None
                }
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
                renew: false,
            }],
            bootnodes: vec![],
        }
    };

    let mut font_def = egui::FontDefinitions::empty();
    font_def.font_data.insert(
        "Nerd-Font".into(),
        egui::FontData::from_static(include_bytes!("../fonts/Nerd Regular Mono.ttf")).tweak(
            egui::FontTweak {
                scale: 1.0,
                y_offset_factor: 0.0,
                y_offset: 0.0,
                baseline_offset_factor: 0.0,
            },
        ),
    );

    font_def
        .families
        .insert(egui::FontFamily::Monospace, vec!["Nerd-Font".to_string()]);
    font_def.families.insert(
        egui::FontFamily::Proportional,
        vec!["Nerd-Font".to_string()],
    );

    egui_context.set_fonts(font_def);

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
            audio_logic_sender,
            logic_audio_receiver,
        );
        logic.run().await;
    }));

    drop(lo);

    {
        let mut app = TheMan::new(gui_logic_receiver, logic_gui_sender);

        let mut repaint_after = None;

        event_loop.run_return(move |event, event_loop, control_flow| {
            //
            match event {
                winit::event::Event::NewEvents(event) => match event {
                    winit::event::StartCause::ResumeTimeReached {
                        start,
                        requested_resume,
                    } => {
                        window.request_redraw();
                        repaint_after = None;
                    }
                    winit::event::StartCause::WaitCancelled {
                        start,
                        requested_resume,
                    } => {
                        if let Some(requested_resume) = requested_resume {
                            control_flow.set_wait_until(requested_resume);
                        }
                    }
                    winit::event::StartCause::Poll => {}
                    winit::event::StartCause::Init => {
                        control_flow.set_wait();
                    }
                },
                winit::event::Event::WindowEvent { window_id, event } => {
                    let res = egui_state.on_event(&egui_context, &event);
                    if !res.consumed {
                        match event {
                            winit::event::WindowEvent::CloseRequested => control_flow.set_exit(),
                            winit::event::WindowEvent::Resized(new_size) => {
                                surface.resize(
                                    &context,
                                    new_size.width.try_into().unwrap(),
                                    new_size.height.try_into().unwrap(),
                                );
                            }
                            _ => {}
                        }
                    }
                    if res.repaint {
                        window.request_redraw()
                    }
                }
                winit::event::Event::DeviceEvent { device_id, event } => {}
                winit::event::Event::UserEvent(_) => {}
                winit::event::Event::Suspended => {}
                winit::event::Event::Resumed => {}
                winit::event::Event::MainEventsCleared => {
                    // app.process_events();
                }
                winit::event::Event::RedrawRequested(window_id) => {
                    unsafe {
                        gl.clear_color(0.0, 0.0, 0.0, 1.0);
                        gl.clear(glow::COLOR_BUFFER_BIT);
                    }
                    let mut raw_input = egui_state.take_egui_input(&window);
                    let size = window.inner_size();
                    raw_input.screen_rect = Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::Vec2 {
                            x: size.width as f32,
                            y: size.height as f32,
                        },
                    ));
                    let output = egui_context.run(raw_input, |ctx| {
                        app.update(ctx);
                    });
                    let primitives = egui_context.tessellate(output.shapes);
                    let PhysicalSize { width, height } = window.inner_size();
                    egui_painter.paint_and_update_textures(
                        [width, height],
                        1.0,
                        &primitives,
                        &output.textures_delta,
                    );
                    surface.swap_buffers(&context);
                    egui_state.handle_platform_output(
                        &window,
                        &egui_context,
                        output.platform_output,
                    );
                    if output.repaint_after < std::time::Duration::from_secs(1440) {
                        if output.repaint_after > std::time::Duration::ZERO {
                            if let Some(repaint_after) = &mut repaint_after {
                                if *repaint_after > output.repaint_after {
                                    *repaint_after = output.repaint_after;
                                }
                            } else {
                                repaint_after = Some(output.repaint_after)
                            }
                        } else {
                            window.request_redraw();
                        }
                    }

                    if let Some(repaint_after) = &repaint_after {
                        control_flow.set_wait_until(std::time::Instant::now() + *repaint_after);
                    }
                }
                winit::event::Event::RedrawEventsCleared => {}
                winit::event::Event::LoopDestroyed => {
                    println!("Loop Destroyed");
                    app.save();
                    app.state.send(Message::ShutDown);
                }
            }
        });
    }
    let audio = Arc::try_unwrap(audio).unwrap().into_inner().unwrap();
    if let Some(audio) = audio {
        audio.await.unwrap()
    }
    let logic = Arc::try_unwrap(logic).unwrap().into_inner().unwrap();
    if let Some(logic) = logic {
        logic.await.unwrap()
    }
}
