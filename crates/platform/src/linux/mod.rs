use std::sync::{Arc, atomic::AtomicU32};

use ashpd::desktop::{
    Session,
    screencast::{self, Screencast},
};
use pipewire::ToPipeWireEvent;
use tokio::sync::Mutex;

use crate::{ID, PlatformEvent, Receiver, Sender, TPlatform, channel};

mod pipewire;

struct Shared {
    pw_sender: ::pipewire::channel::Sender<ToPipeWireEvent>,
    sender: Sender<PlatformEvent>,
    proxy_screencast: Screencast<'static>,
    session_screencast: Mutex<Option<Session<'static, Screencast<'static>>>>,
    counter: AtomicU32,
}

impl Shared {
    pub fn next_id(&self) -> ID {
        let id = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        ID { inner: id }
    }
}

pub struct PlatformPipewire {
    handle: Option<std::thread::JoinHandle<()>>,
    shared: Arc<Shared>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl TPlatform for PlatformPipewire {
    fn init(&mut self) {
        let (sender, receiver) = channel();

        self.shared
            .pw_sender
            .send(ToPipeWireEvent::CreateOutputAuto(0, receiver));
        self.shared.sender.send(PlatformEvent::NewAudioOutput(
            self.shared.next_id(),
            sender,
            "Pipewire AUDIO OUT".into(),
        ));

        let (sender, receiver) = channel();
        self.shared
            .pw_sender
            .send(ToPipeWireEvent::CreateInputAuto(0, sender));
        self.shared.sender.send(PlatformEvent::NewAudioInput(
            self.shared.next_id(),
            receiver,
            "Pipewire AUDIO IN".into(),
        ));
    }

    fn start_screen_share(&mut self) {
        self.task = Some(tokio::spawn(start_screencast_session(self.shared.clone())))
    }

    fn stop_screen_share(&mut self) {
        self.task = Some(tokio::spawn(stop_screencast_session(self.shared.clone())))
    }
}

pub fn init_pipewire_platform() -> Option<(
    Receiver<PlatformEvent>,
    Box<dyn TPlatform + Sync + Send + 'static>,
)> {
    let (pw_sender, pw_receiver) = ::pipewire::channel::channel::<ToPipeWireEvent>();
    let (sender, receiver) = channel();
    let handle = pipewire::start_pipewire(pw_receiver);

    let proxy_screencast = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async move {
            Screencast::new()
                .await
                .expect("Cannot create screencast proxy")
        })
    });

    Some((
        receiver,
        Box::new(PlatformPipewire {
            handle: Some(handle),
            shared: Arc::new(Shared {
                pw_sender,
                sender,
                proxy_screencast,
                session_screencast: Mutex::new(None),
                counter: Default::default(),
            }),
            task: None,
        }),
    ))
}

async fn stop_screencast_session(shared: Arc<Shared>) {
    if let Some(session) = shared.session_screencast.lock().await.take() {
        _ = shared
            .pw_sender
            .send(pipewire::ToPipeWireEvent::RemoveCore(1));
        session.close().await;
    }
}

async fn start_screencast_session(shared: Arc<Shared>) {
    stop_screencast_session(shared.clone()).await;

    let session = shared
        .proxy_screencast
        .create_session()
        .await
        .expect("Cannot create screenshare session");

    *shared.session_screencast.lock().await = Some(session);

    let lok = shared.session_screencast.lock().await;
    let s = lok.as_ref().unwrap();

    if let Ok(_) = shared
        .proxy_screencast
        .select_sources(
            s,
            screencast::CursorMode::Embedded,
            screencast::SourceType::Monitor
                | screencast::SourceType::Window
                | screencast::SourceType::Virtual,
            true,
            None,
            ashpd::desktop::PersistMode::DoNot,
        )
        .await
    {}

    let mut _streams = Vec::default();

    if let Ok(res) = shared.proxy_screencast.start(s, None).await {
        if let Ok(streams) = res.response() {
            for stream in streams.streams() {
                _streams.push(stream.pipe_wire_node_id());
            }
        }
    }

    if let Ok(fd) = shared.proxy_screencast.open_pipe_wire_remote(s).await {
        _ = shared
            .pw_sender
            .send(pipewire::ToPipeWireEvent::ConnectTo(fd));

        for stream in _streams {
            let (sender, receiver) = channel();
            shared
                .pw_sender
                .send(ToPipeWireEvent::CreateVideoStream(1, stream, sender));
            shared.sender.send(PlatformEvent::NewVideoInput(
                shared.next_id(),
                receiver,
                format!("Pipewire VIDEO OUT {stream}"),
            ));
        }
    }
}
