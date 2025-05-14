use std::{
    cell::RefCell,
    collections::HashMap,
    os::fd::{FromRawFd, OwnedFd},
    rc::Rc,
    sync::Arc,
};

use gui_deps::{rand::distributions::uniform::SampleBorrow, *};

use tracing::{error, info, warn};

pub enum ToPipeWireEvent {
    CreateOutputAuto(i32, tokio::sync::mpsc::UnboundedReceiver<f32>),
    CreateInputAuto(i32, tokio::sync::mpsc::UnboundedSender<f32>),
    ConnectTo(OwnedFd),
    RemoveCore(i32),
    CreateVideoStream(
        i32,
        u32,
        tokio::sync::mpsc::UnboundedSender<(u32, u32, Arc<[u8]>)>,
    ),
    Shutdown,
}

pub struct CoreState {
    core: pipewire::core::Core,
    output_streams: Vec<(
        pipewire::stream::Stream,
        pipewire::stream::StreamListener<tokio::sync::mpsc::UnboundedReceiver<f32>>,
    )>,
    input_streams: Vec<(
        pipewire::stream::Stream,
        pipewire::stream::StreamListener<tokio::sync::mpsc::UnboundedSender<f32>>,
    )>,

    input_video_streams: Vec<(
        pipewire::stream::Stream,
        pipewire::stream::StreamListener<tokio::sync::mpsc::UnboundedSender<(u32, u32, Arc<[u8]>)>>,
    )>,
}

pub fn start_pipewire(
    receiver: pipewire::channel::Receiver<ToPipeWireEvent>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("pipewire".into())
        .spawn(move || {
            pipewire::init();

            let main_loop =
                pipewire::main_loop::MainLoop::new(None).expect("Cannot create pipewire main_loop");

            let context = pipewire::context::Context::new(&main_loop)
                .expect("Cannot create pipewire context");

            let core = context
                .connect(None)
                .expect("Cannot create pipewire default core");

            // let output_streams = Rc::<RefCell<Vec<_>>>::default();
            // let input_streams = Rc::<RefCell<Vec<_>>>::default();
            let cores = Rc::<RefCell<HashMap<i32, CoreState>>>::default();

            cores.borrow_mut().insert(
                0,
                CoreState {
                    core,
                    output_streams: vec![],
                    input_streams: vec![],
                    input_video_streams: vec![],
                },
            );

            let _event_listener = {
                let l_main_loop = main_loop.clone();
                receiver.attach(main_loop.loop_(), move |event| match event {
                    ToPipeWireEvent::CreateOutputAuto(core_fd, sample_receiver) => {
                        let mut cores = cores.borrow_mut();
                        if let Some(core_state) = cores.get_mut(&core_fd) {
                            create_output_stream(core_state, sample_receiver);
                        }
                    }
                    ToPipeWireEvent::CreateInputAuto(core_fd, sample_sender) => {
                        let mut cores = cores.borrow_mut();
                        if let Some(core_state) = cores.get_mut(&core_fd) {
                            create_input_stream(core_state, sample_sender);
                        }
                    }
                    ToPipeWireEvent::ConnectTo(fd) => {
                        let Ok(core) = context.connect_fd(fd, None).inspect_err(|err| {
                            error!("Cannot connect to fd, {err}");
                        }) else {
                            return;
                        };

                        cores.borrow_mut().insert(
                            1,
                            CoreState {
                                core,
                                output_streams: vec![],
                                input_streams: vec![],
                                input_video_streams: vec![],
                            },
                        );
                    }
                    ToPipeWireEvent::RemoveCore(core_id) => {
                        let mut cores = cores.borrow_mut();
                        cores.remove(&core_id);
                    }
                    ToPipeWireEvent::CreateVideoStream(core_id, node_id, sender) => {
                        let mut cores = cores.borrow_mut();

                        if let Some(core) = cores.get_mut(&core_id) {
                            create_video_stream(core, node_id, sender);
                        } else {
                            error!("Cannot find core: {core_id}");
                        }
                    }
                    ToPipeWireEvent::Shutdown => {
                        l_main_loop.quit();
                    }
                })
            };

            main_loop.run();
            info!("PipeWire MainLoop Exiting");
        })
        .expect("Cannot create pipewire thread")
}

fn create_input_stream(
    core_state: &mut CoreState,
    sample_sender: tokio::sync::mpsc::UnboundedSender<f32>,
) {
    let stream = pipewire::stream::Stream::new(
        &core_state.core,
        "audio-in",
        pipewire::properties::properties! {
            "media.type" => "Audio",
            "media.category" => "Capture",
            "media.role" => "Music",
        },
    )
    .unwrap();

    let stream_listener = {
        stream
            .add_local_listener_with_user_data(sample_sender)
            .process(|stream, sender| {
                let Some(mut buffer) = stream.dequeue_buffer() else {
                    warn!("Cannot dequeue buffer");
                    return;
                };

                let buf = buffer.datas_mut();
                let Some(dst) = buf.first_mut() else {
                    warn!("No Data in buffer");
                    return;
                };

                let size = dst.chunk().size() as usize;

                let Some(data) = dst.data() else {
                    warn!("No Data");
                    return;
                };

                {
                    let data = unsafe {
                        std::slice::from_raw_parts_mut::<f32>(
                            data.as_mut_ptr().cast(),
                            size / size_of::<f32>(),
                        )
                    };

                    for sample in data {
                        if let Err(err) = sender.send(*sample) {
                            error!("When sending input: {err}");
                        }
                    }
                }
            })
            .register()
            .expect("Cannot create Input Audio Stream")
    };

    let mut params = pipewire::spa::param::audio::AudioInfoRaw::new();
    params.set_format(pipewire::spa::param::audio::AudioFormat::F32LE);
    params.set_channels(2);
    params.set_rate(48000);

    let mut data = vec![0u8; 1024];
    let pod_builder = pipewire::spa::pod::builder::Builder::new(&mut data);
    let pod;
    unsafe {
        pod = pipewire::spa::sys::spa_format_audio_raw_build(
            pod_builder.as_raw_ptr(),
            pipewire::spa::sys::SPA_PARAM_EnumFormat,
            &params.as_raw(),
        );
    }

    stream
        .connect(
            pipewire::spa::utils::Direction::Input,
            None,
            pipewire::stream::StreamFlags::AUTOCONNECT
                | pipewire::stream::StreamFlags::MAP_BUFFERS
                | pipewire::stream::StreamFlags::RT_PROCESS,
            &mut [unsafe { pipewire::spa::pod::Pod::from_raw(pod) }],
        )
        .unwrap();

    core_state.input_streams.push((stream, stream_listener));
}

fn create_output_stream(
    core_state: &mut CoreState,
    sample_receiver: tokio::sync::mpsc::UnboundedReceiver<f32>,
) {
    let Ok(stream) = pipewire::stream::Stream::new(
        &core_state.core,
        "audio-out",
        pipewire::properties::properties! {
            "media.type" => "Audio",
            "media.category" => "Playback",
            "media.role" => "Music",
        },
    )
    .inspect_err(|err| error!("When creating output stream, {err}")) else {
        return;
    };

    let stream_listener = {
        stream
            .add_local_listener_with_user_data(sample_receiver)
            .process(|stream, receiver| {
                let Some(mut buffer) = stream.dequeue_buffer() else {
                    warn!("Cannot dequeue buffer");
                    return;
                };

                let buf = buffer.datas_mut();
                let Some(dst) = buf.first_mut() else {
                    warn!("No Data in buffer");
                    return;
                };

                let Some(data) = dst.data() else {
                    warn!("No Data");
                    return;
                };

                let mut i = 0;
                {
                    let data = unsafe {
                        std::slice::from_raw_parts_mut::<f32>(
                            data.as_mut_ptr().cast(),
                            data.len() / size_of::<f32>(),
                        )
                    };
                    while data.len() > i {
                        if let Ok(sample) = receiver.try_recv() {
                            data[i] = sample;
                            i += 1;
                        } else {
                            break;
                        }
                    }
                }

                let size = i * size_of::<f32>();
                *dst.chunk_mut().offset_mut() = 0;
                *dst.chunk_mut().stride_mut() = size_of::<f32>() as i32;
                *dst.chunk_mut().size_mut() = size as u32;
            })
            .register()
            .expect("Cannot create Output Audio Stream")
    };

    let mut params = pipewire::spa::param::audio::AudioInfoRaw::new();
    params.set_format(pipewire::spa::param::audio::AudioFormat::F32LE);
    params.set_channels(2);
    params.set_rate(48000);

    let mut data = vec![0u8; 1024];
    let pod_builder = pipewire::spa::pod::builder::Builder::new(&mut data);

    let pod;
    unsafe {
        pod = pipewire::spa::sys::spa_format_audio_raw_build(
            pod_builder.as_raw_ptr(),
            pipewire::spa::sys::SPA_PARAM_EnumFormat,
            &params.as_raw(),
        );
    }

    stream
        .connect(
            pipewire::spa::utils::Direction::Output,
            None,
            pipewire::stream::StreamFlags::AUTOCONNECT
                | pipewire::stream::StreamFlags::MAP_BUFFERS
                | pipewire::stream::StreamFlags::RT_PROCESS,
            &mut [unsafe { pipewire::spa::pod::Pod::from_raw(pod) }],
        )
        .unwrap();

    core_state.output_streams.push((stream, stream_listener));
}

fn create_video_stream(
    core_state: &mut CoreState,
    node_id: u32,
    sender: tokio::sync::mpsc::UnboundedSender<(u32, u32, Arc<[u8]>)>,
) {
    let stream = pipewire::stream::Stream::new(
        &core_state.core,
        "video-in",
        pipewire::properties::properties! {
            "media.type" => "Video",
            "media.category" => "Capture",
            "media.role" => "Video",
        },
    )
    .unwrap();

    let size = Rc::new(std::cell::RefCell::new((0u32, 0u32)));

    let size1 = size.clone();
    let size2 = size;

    let stream_listener = {
        stream
            .add_local_listener_with_user_data(sender)
            .param_changed(move |stream, _, a, b| {
                info!("New params: {a}");
                if let Some(pod) = b {
                    let mut video_info = pipewire::spa::param::video::VideoInfoRaw::new();
                    if let Ok(_) = video_info.parse(pod) {
                        let mut s = size1.borrow_mut();
                        s.0 = video_info.size().width;
                        s.1 = video_info.size().height;
                        info!("Video: {video_info:?}");
                    }
                    // pipewire::spa::utils::dict::Di
                }
            })
            .process(move |stream, sender| {
                let Some(mut buffer) = stream.dequeue_buffer() else {
                    warn!("Cannot dequeue buffer");
                    return;
                };

                let buf = buffer.datas_mut();
                let Some(dst) = buf.first_mut() else {
                    warn!("No Data in buffer");
                    return;
                };

                let size = dst.chunk().size() as usize;

                let Some(data) = dst.data() else {
                    warn!("No Data");
                    return;
                };

                let mut data = data[..size].to_vec();
                for slice in data.chunks_exact_mut(4) {
                    if let [b, g, r, a] = slice {
                        std::mem::swap(&mut (*b), &mut (*r));
                    } else {
                        error!("WUT??? no BGRA???");
                    }
                }

                let s = size2.borrow();

                sender.send((s.0, s.1, Arc::from(data)));
            })
            .register()
            .expect("Cannot create Input Video Stream")
    };

    let mut params = pipewire::spa::param::video::VideoInfoRaw::new();
    params.set_format(pipewire::spa::param::video::VideoFormat::BGRA);

    let mut data = vec![0u8; 1024];
    let pod_builder = pipewire::spa::pod::builder::Builder::new(&mut data);
    let pod;
    unsafe {
        pod = pipewire::spa::sys::spa_format_video_raw_build(
            pod_builder.as_raw_ptr(),
            pipewire::spa::sys::SPA_PARAM_EnumFormat,
            &params.as_raw(),
        );
    }

    stream
        .connect(
            pipewire::spa::utils::Direction::Input,
            Some(node_id),
            pipewire::stream::StreamFlags::MAP_BUFFERS | pipewire::stream::StreamFlags::AUTOCONNECT,
            &mut [unsafe { pipewire::spa::pod::Pod::from_raw(pod) }],
        )
        .unwrap();

    core_state
        .input_video_streams
        .push((stream, stream_listener));
}
