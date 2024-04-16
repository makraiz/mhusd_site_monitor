use super::*;

/// Used for sending signals to Tokio thread via mspc channel.  
#[derive(Clone)]
pub enum TokioEvent {
    EventProxy(ContextProxy),
    RefreshSites,
    TimerElapsed,
}

/// Application events.  Events can be sent from Tokio thread via ContextProxy.  
pub enum ViziaEvent {
    TimerIncrement,             // 1 second increments.
    TimerReset,                 // Sent when timer reaches 0.
    PingResponse(PingResponse), // Sent from tokio thread, first string is Key, second string is Value.
    MenuTogglePressed,          // Show/hide menu pane.
    TimerDurationChanged(i32),  // Change the timer duration.  
    RefreshSites,               // Reloads sites.json. 
}

/// Maps sites.json.  Panics if unable to read sites.json or unable to parse the data within the file.  
pub fn read_sites() -> BTreeMap<String, IpAddr> {
    let data = fs::read_to_string(Path::new("sites.json")).expect("Unable to read file");
    serde_json::from_str(&data).expect("Unable to deserialize data")
}

/// Converts data from read_sites into useful data for vizia_main AppData
pub fn sites_to_pings(sites: BTreeMap<String, IpAddr>) -> Vec<PingResponse> {
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

/// Application data / model.  
#[derive(Lens, Clone)]
pub struct AppData {
    pub sites: Vec<PingResponse>,
    pub timer: Timer,
    pub timer_count: i32,
    pub tx: mpsc::Sender<TokioEvent>,
    pub menu_visible: bool,
    pub timer_duration: i32,
    pub current_time: DateTime<Local>,
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
                    self.current_time = Local::now();
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

/// Data structure for site name & ping response.  
#[derive(Lens, Clone, PartialEq, Data)]
pub struct PingResponse {
    pub name: String,
    pub response: String,
    pub is_err: bool,
}

/// Simple data structure for site name & ip address.
pub struct SiteAddress {
    pub name: String,
    pub addr: IpAddr,
}
