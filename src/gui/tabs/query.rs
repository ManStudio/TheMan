use super::Tab;

#[derive(Default)]
pub struct TabQuery {
    id: usize,
    index: usize,
}

impl Tab for TabQuery {
    fn name(&self) -> &str {
        "Query"
    }

    fn update(
        &mut self,
        ui: &mut eframe::egui::Ui,
        state: &mut crate::gui::TheManGuiState,
    ) -> Option<String> {
        let mut iter = state.kademlia_query_progress.iter();
        if let Some((_, query)) = iter.nth(self.index) {
            ui.label(format!("Requests: {}", query.1.num_requests()));
            ui.label(format!("Sucesses: {}", query.1.num_successes()));
            ui.label(format!("Failures: {}", query.1.num_failures()));
            ui.label(format!("Pending: {}", query.1.num_pending()));
            match &query.0 {
                libp2p::kad::QueryResult::Bootstrap(bootstrap) => match bootstrap {
                    Ok(res) => {
                        ui.label("Bootstrap Ok");
                        ui.label(format!("Remaining: {}", res.num_remaining));
                        ui.label(format!("PeerId: {}", res.peer));
                    }
                    Err(err) => {
                        ui.label("Bootstrap Err");
                        ui.label(format!("Err: {:?}", err));
                    }
                },
                libp2p::kad::QueryResult::GetClosestPeers(res) => match res {
                    Ok(res) => {
                        ui.label("GetClosestPeers Ok");
                        ui.label(format!("Key: {:?}", res.key));
                        for peer in res.peers.iter() {
                            ui.label(format!("Peer: {}", peer));
                        }
                    }
                    Err(err) => {
                        ui.label("GetClosesPeers Err");
                        ui.label(format!("{:?}", err.key()));
                    }
                },
                libp2p::kad::QueryResult::GetProviders(res) => match res {
                    Ok(res) => {
                        ui.label("GetProviders Ok");
                        ui.label(format!("{:?}", res));
                    }
                    Err(err) => {
                        ui.label("GetProviders Err");
                        ui.label(format!("Err: {err:?}"));
                    }
                },
                libp2p::kad::QueryResult::StartProviding(res) => match res {
                    Ok(res) => {
                        ui.label("StartProviding");
                        ui.label(format!("{res:?}"));
                    }
                    Err(err) => {
                        ui.label("StartProviding Err");
                        ui.label(format!("Err: {err:?}"));
                    }
                },
                libp2p::kad::QueryResult::RepublishProvider(res) => match res {
                    Ok(res) => {
                        ui.label("RepublishProvider Ok");
                        ui.label(format!("{res:?}"));
                    }
                    Err(err) => {
                        ui.label("RepublishProvider Err");
                        ui.label(format!("Err {err:?}"));
                    }
                },
                libp2p::kad::QueryResult::GetRecord(res) => match res {
                    Ok(res) => {
                        ui.label("GetRecord Ok");
                        ui.label(format!("{res:?}"));
                    }
                    Err(err) => {
                        ui.label("GetRecord Err");
                        ui.label(format!("Err: {err:?}"));
                    }
                },
                libp2p::kad::QueryResult::PutRecord(res) => match res {
                    Ok(res) => {
                        ui.label("PutRecord");
                        ui.label(format!("{res:?}"));
                    }
                    Err(err) => {
                        ui.label("PutRecord Err");
                        ui.label(format!("Err: {err:?}"));
                    }
                },
                libp2p::kad::QueryResult::RepublishRecord(res) => match res {
                    Ok(res) => {
                        ui.label("RepublishRecord Ok");
                        ui.label(format!("{res:?}"));
                    }
                    Err(err) => {
                        ui.label("RepublishRecord Err");
                        ui.label(format!("Err: {err:?}"));
                    }
                },
            }
        }
        None
    }

    fn hidden(&self) -> bool {
        true
    }

    fn recive(&mut self, message: String) {
        if let Ok(index) = message.parse::<usize>() {
            self.index = index;
        }
    }

    fn clone_box(&self) -> Box<dyn Tab> {
        Box::<Self>::default()
    }

    fn id(&self) -> usize {
        self.id
    }

    fn set_id(&mut self, id: usize) {
        self.id = id;
    }
}
