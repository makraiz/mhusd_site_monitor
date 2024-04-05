use std::{collections::BTreeMap, fs, net::IpAddr, path::Path};

use rand::random;
use std::sync::mpsc;
use surge_ping::{Client, Config, IcmpPacket, PingIdentifier, PingSequence, ICMP};
use vizia::prelude::*;

fn main() {
    // Set up communications channel for data to get from GUI thread to tokio thread.
    let (vizia_tx, tokio_rx) = mpsc::channel::<TokioEvent>(); // Listens for data/events from GUI thread.

    // Read site data, make a copy for each thread to own.
    let site_data = read_sites();
    let site_data2 = sites_to_pings(site_data.clone());

    // Spawn the tokio thread
    let _tokio_handle = std::thread::spawn(|| tokio_main(tokio_rx, site_data));

    // GUI runs on main thread.
    vizia_main(vizia_tx, site_data2); // Blocking.
}

/// Used for sending signals to Tokio thread via mspc channel.  
#[derive(Clone)]
enum TokioEvent {
    EventProxy(ContextProxy),
    TimerElapsed,
}

/// Maps sites.json.  Panics if unable to read sites.json or unable to parse the data within the file.  
fn read_sites() -> BTreeMap<String, IpAddr> {
    let data = fs::read_to_string(Path::new("sites.json")).expect("Unable to read file");
    serde_json::from_str(&data).expect("Unable to deserialize data")
}

/// Application events.  Events can be sent from Tokio thread via ContextProxy.  
enum ViziaEvent {
    TimerIncrement,             // 1 second increments.
    TimerReset,                 // Sent when timer reaches 0.
    PingResponse(PingResponse), // Sent from tokio thread, first string is Key, second string is Value.
}

/// Application data / model.  
#[derive(Lens)]
struct AppData {
    sites: Vec<PingResponse>,
    timer: Timer,
    timer_count: i32,
    tx: mpsc::Sender<TokioEvent>,
}
impl Model for AppData {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|app_event, _| {
            match app_event {
                ViziaEvent::TimerIncrement => {
                    self.timer_count -= 1;
                    if self.timer_count <= 0 {
                        cx.emit(ViziaEvent::TimerReset);
                    }
                }
                ViziaEvent::TimerReset => {
                    let _ = self.tx.send(TokioEvent::TimerElapsed); // TODO: Handle potential errors.
                    self.timer_count = 30;
                }
                ViziaEvent::PingResponse(response) => {
                    if let Some(i) = self
                        .sites
                        .iter()
                        .position(|site| site.name == response.name)
                    {
                        self.sites[i] = response.clone();
                    } else {
                        self.sites.push(response.clone());
                    }
                }
            }
        })
    }
}

/// Simple data structure for site name & ip address.
struct SiteAddress {
    name: String,
    addr: IpAddr,
}

/// Data structure for site name & ping response.  
#[derive(Lens, Clone)]
struct PingResponse {
    name: String,
    response: String,
}

/// Initates the runtime loop.  Must send ContextProxy first over mpsc channel, else panic!  
#[tokio::main] // Creates the runtime for us.
async fn tokio_main(rx: mpsc::Receiver<TokioEvent>, sites: BTreeMap<String, IpAddr>) {
    const DEF_TIMEOUT: u64 = 4;
    const DEF_PAYLOAD: [u8; 256] = [0; 256];

    // Create the ping clients.
    let client_v4 = Client::new(&Config::default()).expect("Couldn't create IPv4 Client!");
    let client_v6 = Client::new(&Config::builder().kind(ICMP::V6).build())
        .expect("Couldn't create IPv6 Client!");

    // Get the context proxy.
    let cx = match rx.recv() {
        // Sleeps thread until we get something from the channel.
        Ok(e) => match e {
            TokioEvent::EventProxy(cx) => cx,
            _ => panic!("Received event other than EventProxy first!"), // User error, rewrite your code.
        },
        Err(_e) => panic!("Channel was closed before receiving any values!"), // Sender was dropped, something went wrong.  Should be unreachable.
    };

    // Start the loop.
    loop {
        match rx.recv() {
            // Blocks until something is present
            Ok(e) => {
                // Handle the event
                match e {
                    TokioEvent::EventProxy(_) => panic!("Received another EventProxy!"), // We should not ever receive a second proxy.
                    TokioEvent::TimerElapsed => {
                        // Loop through all the sites.
                        for (name, address) in sites.iter() {
                            // Create a SiteAddress for passing
                            let site = SiteAddress {
                                name: name.clone(),
                                addr: address.clone(),
                            };
                            match address {
                                // Check address type and send the appropriate client to the task
                                IpAddr::V4(_) => {
                                    tokio::spawn(ping(
                                        cx.clone(),
                                        client_v4.clone(),
                                        site,
                                        DEF_TIMEOUT,
                                        DEF_PAYLOAD,
                                    ));
                                }
                                IpAddr::V6(_) => {
                                    tokio::spawn(ping(
                                        cx.clone(),
                                        client_v6.clone(),
                                        site,
                                        DEF_TIMEOUT,
                                        DEF_PAYLOAD,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            Err(_e) => panic!("Window was closed, shutting down."),
        }
    }
}

/// Ping a site.  Sends a PingResponse back to the GUI thread.  
async fn ping(
    mut cx: ContextProxy,
    client: Client,
    site: SiteAddress,
    timeout: u64,
    payload: [u8; 256],
) {
    // Create the pinger.
    let mut pinger = client.pinger(site.addr, PingIdentifier(random())).await;
    pinger.timeout(Duration::from_secs(timeout));

    // Get the result, send as event back to GUI.
    let _ = match pinger.ping(PingSequence(random()), &payload).await {
        Ok((IcmpPacket::V4(_packet), dur)) => cx.emit(ViziaEvent::PingResponse(PingResponse {
            name: site.name,
            response: format!("{dur:0.2?}"),
        })),
        Ok((IcmpPacket::V6(_packet), dur)) => cx.emit(ViziaEvent::PingResponse(PingResponse {
            name: site.name,
            response: format!("{dur:0.2?}"),
        })),
        Err(e) => cx.emit(ViziaEvent::PingResponse(PingResponse {
            name: site.name,
            response: format!("{e:?}"),
        })),
    };
}

fn vizia_main(tx: mpsc::Sender<TokioEvent>, sites: Vec<PingResponse>) {
    // Spin up the GUI.
    let _ = Application::new(move |cx| {
        // Create & send ContextProxy to Tokio thread for event messaging.
        let proxy = cx.get_proxy();
        let _ = tx.send(TokioEvent::EventProxy(proxy));

        // Create a timer that sends an event every second to update the gui
        let timer = cx.add_timer(Duration::from_secs(1), None, |cx, action| match action {
            TimerAction::Tick(_) => cx.emit(ViziaEvent::TimerIncrement),
            _ => {}
        });

        // Create the data model for the GUI context.
        AppData {
            sites,
            timer,
            timer_count: 30,
            tx,
        }
        .build(cx);
        cx.start_timer(timer);

        // Window Layout
        HStack::new(cx, |cx| {
            // Left side, site names and responses.
            VStack::new(cx, |cx| {
                List::new(cx, AppData::sites, |cx, _, site| {
                    HStack::new(cx, |cx| {
                        Label::new(cx, site.then(PingResponse::name))
                            .child_left(Pixels(20.0))
                            .child_right(Stretch(1.0));
                        Label::new(cx, site.then(PingResponse::response))
                            .child_left(Stretch(1.0))
                            .child_right(Pixels(20.0));
                    })
                    .col_between(Stretch(1.0));
                })
                .row_between(Pixels(20.0));
            })
            .child_space(Pixels(20.0));

            // Right side, timer countdown and controls (eventually).
            VStack::new(cx, |cx| {
                HStack::new(cx, |_cx| {}); // Exists to take top spot in Vstack
                HStack::new(cx, |cx| {
                    Label::new(cx, "Ping Interval")
                        .child_top(Stretch(1.0))
                        .child_bottom(Pixels(0.0))
                        .child_left(Pixels(20.0))
                        .child_right(Stretch(1.0));
                    Label::new(cx, AppData::timer_count)
                        .child_top(Stretch(1.0))
                        .child_bottom(Pixels(0.0))
                        .child_left(Stretch(1.0))
                        .child_right(Pixels(20.0));
                })
                .col_between(Stretch(1.0))
                .child_space(Pixels(20.0));
            })
            .row_between(Stretch(1.0))
            .child_space(Pixels(20.0));
        });
    })
    .run();
}

/// Converts data from read_sites into useful data for vizia_main AppData
fn sites_to_pings(sites: BTreeMap<String, IpAddr>) -> Vec<PingResponse> {
    let mut map = Vec::new();
    for (name, _) in sites {
        map.push(PingResponse {
            name,
            response: String::from("Pending"),
        });
    }
    map
}
