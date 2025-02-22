use std::collections::BTreeMap;

use iroh::NodeId;
use iroh_blobs::Hash;
use serde::{Deserialize, Serialize};
use tracing::error;

#[derive(Clone, Serialize, Deserialize)]
pub struct Account {
    name: String,
    secret: iroh::SecretKey,
    #[serde(default)]
    known_as: BTreeMap<Hash, String>,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Data {
    #[serde(default)]
    accounts: Vec<Account>,
    #[serde(default)]
    knows_node_id: Vec<NodeId>,
}

impl Data {
    pub fn load() -> Data {
        let Ok(data) = std::fs::read("the-man.cbor") else {
            error!("Cannot read file, the-man.cbor");
            return Data::default();
        };

        let Ok(data) = ciborium::from_reader::<Data, _>(std::io::Cursor::new(data)) else {
            error!("Cannot parse data from file, the-man.cbor");
            return Data::default();
        };

        data
    }

    pub fn save(&self) {
        let file =
            std::fs::File::create("the-man.cbor").expect("Cannot create or open the the-man.cbor");
        ciborium::into_writer(&self, file).expect("Cannot serilize data");
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("the_man=trace".parse().unwrap()),
        )
        .init();

    let Ok(backend) = std::env::var("GUI") else {
        eprintln!("Gui backend is not set!");
        eprintln!("GUI=eframe");
        #[cfg(feature = "gui_iced")]
        eprintln!("GUI=iced");
        return;
    };

    let data = Data::load();
    match backend.trim() {
        "eframe" => run_egui(data),
        #[cfg(feature = "gui_iced")]
        "iced" => run_iced(data),
        _ => {
            eprintln!("Invalid backend {backend}");
        }
    }
}

mod backend_eframe;

fn run_egui(data: Data) {
    use backend_eframe::App;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = runtime.enter();
    eframe::run_native(
        "The Man",
        eframe::NativeOptions::default(),
        Box::new(|ctx| Ok(Box::new(App::new(&ctx.egui_ctx, data)))),
    )
    .unwrap();
}

#[cfg(feature = "gui_iced")]
mod backend_iced;

#[cfg(feature = "gui_iced")]
fn run_iced(data: Data) {
    use backend_iced::TheMan;
    use iced::Task;

    let app = iced::application("TheMan", TheMan::update, TheMan::view)
        .subscription(TheMan::subscription);

    app.run_with(|| (TheMan::new(data), Task::none())).unwrap();
}
