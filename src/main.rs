#![windows_subsystem = "windows"]
pub mod model;
pub mod views;
pub mod worker;

pub use crate::model::*;
pub use crate::views::*;
pub use crate::worker::*;

pub use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    net::IpAddr,
    path::Path,
    sync::mpsc,
};

pub use chrono::{DateTime, Local};
pub use rand::random;
pub use surge_ping::{Client, Config, IcmpPacket, PingIdentifier, PingSequence, ICMP};
pub use vizia::prelude::*;

fn main() {
    // Set up communications channel for data to get from GUI thread to tokio thread.
    let (vizia_tx, tokio_rx) = mpsc::channel::<TokioEvent>(); // Listens for data/events from GUI thread.;

    // Spawn the tokio thread
    let _tokio_handle = std::thread::spawn(|| tokio_main(tokio_rx));

    // GUI blocks on main thread.
    vizia_main(vizia_tx);
}
