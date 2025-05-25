use std::time::{Duration, Instant};

use super::Pane;

struct FormatedNumber(u64);

impl std::fmt::Display for FormatedNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            x if x > 1000u64.pow(4) => {
                f.write_fmt(format_args!("{:0.2}Tb", (x as f64) / 1000u64.pow(4) as f64))
            }
            x if x > 1000u64.pow(3) => {
                f.write_fmt(format_args!("{:0.2}Gb", (x as f64) / 1000u64.pow(3) as f64))
            }
            x if x > 1000u64.pow(2) => {
                f.write_fmt(format_args!("{:0.2}Mb", (x as f64) / 1000u64.pow(2) as f64))
            }
            x if x > 1000 => f.write_fmt(format_args!("{:0.2}Kb", (x as f64) / 1000.)),
            x => f.write_fmt(format_args!("{x}b")),
        }
    }
}

pub struct PaneMetrics {
    auto_refresh: bool,

    updated: Instant,
    last_total_send: u64,
    last_total_recv_data: u64,

    per_s_total_send: u64,
    per_s_total_recv_data: u64,
}

impl Default for PaneMetrics {
    fn default() -> Self {
        Self {
            auto_refresh: true,
            updated: Instant::now(),
            last_total_send: 0,
            last_total_recv_data: 0,
            per_s_total_send: 0,
            per_s_total_recv_data: 0,
        }
    }
}

impl Pane for PaneMetrics {
    fn name(&self, account: &crate::Account) -> String {
        String::from("Metrics")
    }

    fn ui(
        &mut self,
        ui: &mut eframe::egui::Ui,
        context: &mut crate::gui::Context,
        the_man: &mut the_man::TheMan,
        account: &mut crate::Account,
    ) {
        let metrics = the_man.metrics();
        let send_ipv4 = metrics.magicsock.send_ipv4.get();
        let send_ipv6 = metrics.magicsock.send_ipv6.get();
        let send_relay = metrics.magicsock.send_relay.get();
        let send_total = send_ipv4 + send_ipv6 + send_relay;

        let recv_data_ipv4 = metrics.magicsock.recv_data_ipv4.get();
        let recv_data_ipv6 = metrics.magicsock.recv_data_ipv6.get();
        let recv_data_relay = metrics.magicsock.recv_data_relay.get();
        let recv_data_total = recv_data_ipv4 + recv_data_ipv6 + recv_data_relay;

        ui.checkbox(&mut self.auto_refresh, "Auto Refersh");

        if self.auto_refresh {
            ui.ctx().request_repaint();
        }

        ui.label(format!("Send IPV4: {}", FormatedNumber(send_ipv4)));
        ui.label(format!("Send IPV6: {}", FormatedNumber(send_ipv6)));
        ui.label(format!("Send relay: {}", FormatedNumber(send_relay)));
        ui.label(format!("Send: {}", FormatedNumber(send_total)));
        ui.separator();

        ui.label(format!(
            "Recv Data IPV4: {}",
            FormatedNumber(recv_data_ipv4)
        ));
        ui.label(format!(
            "Recv Data IPV6: {}",
            FormatedNumber(recv_data_ipv6)
        ));
        ui.label(format!(
            "Recv Data relay: {}",
            FormatedNumber(recv_data_relay)
        ));
        ui.label(format!("Recv Data: {}", FormatedNumber(recv_data_total),));
        ui.separator();

        if self.updated.elapsed() > Duration::from_secs(1) {
            self.per_s_total_send = send_total - self.last_total_send;
            self.per_s_total_recv_data = recv_data_total - self.last_total_recv_data;
            self.last_total_send = send_total;
            self.last_total_recv_data = recv_data_total;
            self.updated = Instant::now();
        }

        ui.label(format!("Send: {}ps", FormatedNumber(self.per_s_total_send)));
        ui.label(format!(
            "Recv Data: {}ps",
            FormatedNumber(self.per_s_total_recv_data)
        ));
    }
}
