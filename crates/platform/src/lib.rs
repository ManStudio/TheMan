use std::sync::Arc;

#[cfg(target_os = "linux")]
mod linux;

pub struct Sender<T> {
    inner: tokio::sync::mpsc::UnboundedSender<T>,
}

impl<T> Sender<T> {
    pub fn send(&self, value: T) {
        _ = self.inner.send(value);
    }
}

pub struct Receiver<T> {
    inner: tokio::sync::mpsc::UnboundedReceiver<T>,
}

impl<T> Receiver<T> {
    pub async fn recv(&mut self) -> Option<T> {
        self.inner.recv().await
    }

    pub fn try_recv(&mut self) -> Option<T> {
        return self.inner.try_recv().ok();
    }
}

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel::<T>();
    (Sender { inner: sender }, Receiver { inner: receiver })
}

pub struct ID {
    inner: u32,
}

impl std::fmt::Display for ID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.inner, f)
    }
}

pub enum PlatformEvent {
    NewAudioOutput(ID, Sender<f32>, String),
    RemovedAudioOutput(ID),
    NewAudioInput(ID, Receiver<f32>, String),
    RemovedAudioInput(ID),
    NewVideoInput(ID, Receiver<(u32, u32, Arc<[u8]>)>, String),
    RemovedVideoInput(ID),
}

pub trait TPlatform {
    fn init(&mut self);

    fn start_screen_share(&mut self);
    fn stop_screen_share(&mut self);
}

pub fn init_platform() -> Option<(
    Receiver<PlatformEvent>,
    Box<dyn TPlatform + Sync + Send + 'static>,
)> {
    #[cfg(target_os = "linux")]
    if let Some(pipewire_platform) = linux::init_pipewire_platform() {
        return Some(pipewire_platform);
    }

    return None;
}
