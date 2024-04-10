#![windows_subsystem = "windows"]

use std::{collections::BTreeMap, fs, net::IpAddr, path::Path};

use rand::random;
use std::sync::mpsc;
use surge_ping::{Client, Config, IcmpPacket, PingIdentifier, PingSequence, ICMP};
use vizia::prelude::*;

fn main() {
    // Set up communications channel for data to get from GUI thread to tokio thread.
    let (vizia_tx, tokio_rx) = mpsc::channel::<TokioEvent>(); // Listens for data/events from GUI thread.;

    // Spawn the tokio thread
    let _tokio_handle = std::thread::spawn(|| tokio_main(tokio_rx));

    // GUI blocks on main thread.
    vizia_main(vizia_tx);
}

/// Used for sending signals to Tokio thread via mspc channel.  
#[derive(Clone)]
enum TokioEvent {
    EventProxy(ContextProxy),
    RefreshSites,
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
    MenuTogglePressed,          // Show/hide menu pane.
    TimerDurationChanged(i32),  // Change the timer duration.  
    RefreshSites,               // Reloads sites.json. 
}

/// Application data / model.  
#[derive(Lens, Clone)]
struct AppData {
    sites: Vec<PingResponse>,
    timer: Timer,
    timer_count: i32,
    tx: mpsc::Sender<TokioEvent>,
    menu_visible: bool,
    timer_duration: i32,
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
                    self.timer_count = self.timer_duration;
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
                ViziaEvent::MenuTogglePressed => {
                    self.menu_visible = !self.menu_visible
                }
                ViziaEvent::TimerDurationChanged(t) => {
                    self.timer_duration = *t;
                }
                ViziaEvent::RefreshSites => {
                    self.sites = sites_to_pings(read_sites());
                    let _ = self.tx.send(TokioEvent::RefreshSites);
                    cx.emit(ViziaEvent::TimerReset);
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
#[derive(Lens, Clone, PartialEq, Data)]
struct PingResponse {
    name: String,
    response: String,
    is_err: bool,
}

/// Initates the runtime loop.  Must send ContextProxy first over mpsc channel, else panic!  
#[tokio::main] // Creates the runtime for us.
async fn tokio_main(rx: mpsc::Receiver<TokioEvent>) {
    const DEF_TIMEOUT: u64 = 4;
    const DEF_PAYLOAD: [u8; 256] = [0; 256];
    let mut sites: BTreeMap<String, IpAddr> = read_sites();

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
                    TokioEvent::RefreshSites => sites = read_sites(),  // Recieved a signal to update the sites.  
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
                                        &DEF_PAYLOAD,
                                    ));
                                }
                                IpAddr::V6(_) => {
                                    tokio::spawn(ping(
                                        cx.clone(),
                                        client_v6.clone(),
                                        site,
                                        DEF_TIMEOUT,
                                        &DEF_PAYLOAD,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            Err(_e) => {},
        }
    }
}

/// Ping a site.  Sends a PingResponse back to the GUI thread.  
async fn ping(
    mut cx: ContextProxy,
    client: Client,
    site: SiteAddress,
    timeout: u64,
    payload: &[u8],
) {
    // Create the pinger.
    let mut pinger = client.pinger(site.addr, PingIdentifier(random())).await;
    pinger.timeout(Duration::from_secs(timeout));

    // Get the result, send as event back to GUI.
    let _ = match pinger.ping(PingSequence(random()), &payload).await {
        Ok((IcmpPacket::V4(_packet), dur)) => cx.emit(ViziaEvent::PingResponse(PingResponse {
            name: site.name,
            response: format!("{dur:0.2?}"),
            is_err: false,
        })),
        Ok((IcmpPacket::V6(_packet), dur)) => cx.emit(ViziaEvent::PingResponse(PingResponse {
            name: site.name,
            response: format!("{dur:0.2?}"),
            is_err: false,
        })),
        Err(e) => {
            let msg = match e {
                surge_ping::SurgeError::Timeout { seq: _ } => format!("Timeout"),
                _ => format!("{e}"),
            };
            cx.emit(ViziaEvent::PingResponse(PingResponse {
                name: site.name,
                response: msg,
                is_err: true,
            }))
        }, 
    };
}

fn vizia_main(tx: mpsc::Sender<TokioEvent>) {
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

        // First round of pings.
        let _ = tx.send(TokioEvent::TimerElapsed);

        let sites = sites_to_pings(read_sites());

        // Create the data model for the GUI context.
        AppData {
            sites,
            timer,
            timer_count: 30,
            tx,
            menu_visible: false,
            timer_duration: 30,
        }
        .build(cx);
        
        cx.start_timer(timer);

        cx.add_stylesheet(include_style!("style.css")).expect("Failed to load style sheet!");

        // Window Layout
        HStack::new(cx, |cx| {
            // Left side, site names and responses.
            List::new(cx, AppData::sites, |cx, _, site| {
                HStack::new(cx, |cx| {
                    Label::new(cx, site.then(PingResponse::name))
                        .class("siteName");
                    Label::new(cx, site.then(PingResponse::response))
                        .class("siteResponse");
                })
                .col_between(Stretch(1.0))
                .class("siteRow")
                .toggle_class("siteRowError", site.then(PingResponse::is_err));                    
            })
            .class("leftPane");

            // Right side, timer countdown and controls (eventually).
            VStack::new(cx, |cx| {
                HStack::new(cx, |cx| {
                    Element::new(cx);  // Exists to take up space.
                    Label::new(cx, "Show Controls ")
                    .class("menuToggleLabel");
                    Switch::new(cx, AppData::menu_visible)
                    .on_toggle(|cx| cx.emit(ViziaEvent::MenuTogglePressed))
                    .class("menuToggleButton");
                })
                .class("menuButtonBar");
                HStack::new(cx, |cx| {
                    Binding::new(cx, AppData::menu_visible, |cx, show| {
                        if show.get(cx) {
                            VStack::new(cx, |cx| {
                                
                                HStack::new(cx, |cx| { // Timer interval control
                                    Element::new(cx);  // Exists to take up space.
                                    Label::new(cx, "Refresh interval: ")
                                    .class("menuInputLabel");
                                    Textbox::new(cx, AppData::timer_duration)
                                    .on_submit(|ex, text, _| {
                                        ex.emit(ViziaEvent::TimerDurationChanged(text))
                                    })
                                    .class("menuInput");
                                }).class("menuInputRow");

                                HStack::new(cx, |cx| { // Refresh now button
                                    Element::new(cx);  // Exists to take up space. 
                                    Button::new(cx, |cx| {
                                        Label::new(cx, "Refresh now")
                                    })
                                    .on_press(|ex| ex.emit(ViziaEvent::TimerReset))
                                    .class("menuInput");
                                })
                                .class("menuInputRow");

                                HStack::new(cx, |cx| { // Reload sites button
                                    Element::new(cx);  // Exists to take up space. 
                                    Button::new(cx, |cx| {
                                        Label::new(cx, "Reload sites")
                                    })
                                    .on_press(|ex| ex.emit(ViziaEvent::RefreshSites))
                                    .class("menuInput");
                                })
                                .class("menuInputRow");


                            })
                            .class("menuPane");
                        }
                    });   
                })
                .class("menuPaneContainer");     
                HStack::new(cx, |cx| {
                    Label::new(cx, "Next refresh in:")
                        .class("timerLabel");
                    Label::new(cx, AppData::timer_count)
                        .class("timerCount");
                })
                .class("timerPane")
                .col_between(Stretch(1.0));
            })
            .class("rightPane")
            .row_between(Stretch(1.0));
        })
        .class("windowBody");
    })
    .title("MHUSD Site Monitor")
    .run();
}

/// Converts data from read_sites into useful data for vizia_main AppData
fn sites_to_pings(sites: BTreeMap<String, IpAddr>) -> Vec<PingResponse> {
    let mut map = Vec::new();
    for (name, _) in sites {
        map.push(PingResponse {
            name,
            response: String::from("Pending..."),
            is_err: false,
        });
    }
    map
}
