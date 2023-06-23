use super::Tab;

#[derive(Default, Clone)]
pub struct TabSwarmStatus {
    id: usize,
}

impl Tab for TabSwarmStatus {
    fn name(&self) -> &str {
        "Swarm Status"
    }

    fn update(
        &mut self,
        ui: &mut egui::Ui,
        state: &mut crate::gui::TheManGuiState,
    ) -> Option<String> {
        if let Some(kademlia_status) = &state.kademlia_status {
            ui.label(format!("Peers: {}", kademlia_status.num_peers()));
            let conn = kademlia_status.connection_counters();
            ui.label(format!("Connections: {}", conn.num_connections()));
            ui.label(format!("Pending: {}", conn.num_pending()));
            ui.label(format!("Pending incoming: {}", conn.num_pending_incoming()));
            ui.label(format!("Pending outgoing: {}", conn.num_pending_outgoing()));
            ui.label(format!("Established: {}", conn.num_established()));
            ui.label(format!(
                "Established incoming: {}",
                conn.num_established_incoming()
            ));
            ui.label(format!(
                "Established outgoing: {}",
                conn.num_established_outgoing()
            ));
            ui.spinner();
        }

        if ui
            .checkbox(&mut state.bootstraping, "Bootstraping")
            .changed()
        {
            state.send(crate::logic::message::Message::BootstrapSet(
                state.bootstraping,
            ));
        }
        None
    }

    fn hidden(&self) -> bool {
        false
    }

    fn clone_box(&self) -> Box<dyn Tab> {
        Box::new(self.clone())
    }

    fn id(&self) -> usize {
        self.id
    }

    fn set_id(&mut self, id: usize) {
        self.id = id;
    }

    fn recive(&mut self, _message: String) {}
}
