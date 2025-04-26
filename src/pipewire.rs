use std::{cell::RefCell, rc::Rc};

use tracing::{error, info, warn};

pub enum ToPipeWireEvent {
    CreateOutput(tokio::sync::mpsc::UnboundedReceiver<f32>),
    CreateInput(tokio::sync::mpsc::UnboundedSender<f32>),
    Shutdown,
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

            let output_streams = Rc::<RefCell<Vec<_>>>::default();
            let input_streams = Rc::<RefCell<Vec<_>>>::default();

            let _event_listener = {
                let l_main_loop = main_loop.clone();
                receiver.attach(main_loop.loop_(), move |event| match event {
                    ToPipeWireEvent::CreateOutput(sample_receiver) => {
                        let Ok(stream) = pipewire::stream::Stream::new(
                            &core,
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

                        output_streams.borrow_mut().push((stream, stream_listener));
                    }
                    ToPipeWireEvent::CreateInput(sample_sender) => {
                        let stream = pipewire::stream::Stream::new(
                            &core,
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

                        input_streams.borrow_mut().push((stream, stream_listener));
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
