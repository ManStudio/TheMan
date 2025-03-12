use std::collections::BTreeMap;

use crate::{base64_deserialize, base64_serialize, protocol::Ticket};

pub struct CommandData {
    ticket: Ticket,
    alt: String,
}

pub enum CommandAuto {
    Start {
        idx: u32,
        codec_name: String,
        codec_settings: BTreeMap<String, String>,
    },
    Play {
        idx: u32,
        ticket: Ticket,
    },
    Stop {
        idx: u32,
    },
}

pub enum Command {
    Data(CommandData),
    Auto(CommandAuto),
}

impl std::str::FromStr for Command {
    type Err = ();

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if !text.starts_with('/') {
            return Err(());
        }

        let text = &text[1..];

        let mut iterator = text.split(' ');

        let Some(command) = iterator.next() else {
            return Err(());
        };

        match command {
            "data" => {
                let Some(ticket_data) = iterator.next() else {
                    return Err(());
                };

                let Ok(ticket) = base64_deserialize::<Ticket>(ticket_data) else {
                    return Err(());
                };

                let mut alt = iterator.fold(String::default(), |mut acc, segment| {
                    acc.push_str(segment);
                    acc.push(' ');
                    acc
                });

                if alt.ends_with(' ') {
                    alt.pop();
                }

                Ok(Self::Data(CommandData { ticket, alt }))
            }
            "auto" => {
                let Some(subcommand) = iterator.next() else {
                    return Err(());
                };

                match subcommand {
                    "start" => {
                        let Some(idx_text) = iterator.next() else {
                            return Err(());
                        };

                        let Ok(idx) = u32::from_str(idx_text) else {
                            return Err(());
                        };

                        let Some(codec_name) = iterator.next() else {
                            return Err(());
                        };

                        let codec_name = codec_name.to_owned();

                        let Some(codec_settings_text) = iterator.next() else {
                            return Err(());
                        };

                        let Ok(codec_settings) =
                            base64_deserialize::<BTreeMap<String, String>>(codec_settings_text)
                        else {
                            return Err(());
                        };

                        Ok(Self::Auto(CommandAuto::Start {
                            idx,
                            codec_name,
                            codec_settings,
                        }))
                    }
                    "play" => {
                        let Some(idx_text) = iterator.next() else {
                            return Err(());
                        };

                        let Ok(idx) = u32::from_str(idx_text) else {
                            return Err(());
                        };

                        let Some(ticket_text) = iterator.next() else {
                            return Err(());
                        };

                        let Ok(ticket) = base64_deserialize::<Ticket>(ticket_text) else {
                            return Err(());
                        };

                        Ok(Self::Auto(CommandAuto::Play { idx, ticket }))
                    }
                    "stop" => {
                        let Some(idx_text) = iterator.next() else {
                            return Err(());
                        };

                        let Ok(idx) = u32::from_str(idx_text) else {
                            return Err(());
                        };

                        Ok(Self::Auto(CommandAuto::Stop { idx }))
                    }
                    _ => Err(()),
                }
            }
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Command::Data(CommandData { ticket, alt }) => f.write_fmt(format_args!(
                "/data {} {alt}",
                base64_serialize(ticket).unwrap()
            )),
            Command::Auto(command_auto) => match command_auto {
                CommandAuto::Start {
                    idx,
                    codec_name,
                    codec_settings,
                } => f.write_fmt(format_args!(
                    "/auto start {idx} {codec_name} {}",
                    base64_serialize(codec_settings).unwrap()
                )),
                CommandAuto::Play { idx, ticket } => f.write_fmt(format_args!(
                    "/auto play {idx} {}",
                    base64_serialize(ticket).unwrap()
                )),
                CommandAuto::Stop { idx } => f.write_fmt(format_args!("/auto stop {idx}")),
            },
        }
    }
}
