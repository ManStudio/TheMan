use std::collections::{BTreeMap, BTreeSet};

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
    #[serde(default)]
    known_nodes: BTreeSet<NodeId>,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Data {
    #[serde(default)]
    accounts: Vec<Account>,
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

mod gui;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("the_man=trace".parse().unwrap()),
        )
        .with_file(true)
        .with_line_number(true)
        .init();

    let data = Data::load();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = runtime.enter();
    eframe::run_native(
        "The Man",
        eframe::NativeOptions::default(),
        Box::new(|ctx| Ok(Box::new(gui::App::new(&ctx.egui_ctx, data)))),
    )
    .unwrap();
}
