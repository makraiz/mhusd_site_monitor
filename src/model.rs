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
    PingResponse(PingResponse), // Sent from tokio thread.
    MenuTogglePressed,          // Show/hide menu pane.
    TimerDurationChanged(i32),  // Change the timer duration.
    RefreshSites,               // Reloads sites.json.
    AverageTogglePressed,       // Toggle between display averages, current ping.
}

/// Populates a Vec of SiteAverages
pub fn start_history(sites: &Vec<PingResponse>) -> Vec<SiteAverage> {
    let mut sites_averages = Vec::new();
    for site in sites {
        sites_averages.push(SiteAverage::new(site.name.clone()));
    }
    sites_averages
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
            response: None,
            is_err: true,
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
    pub show_average: bool,
    pub history: Vec<SiteAverage>,
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
                    if self.show_average {
                        if response.is_err {
                            // Discard error results.
                            return;
                        }
                        if let Some(pos) = self.history.iter().position(|h| h.name == response.name)
                        {
                            self.history
                                .get_mut(pos)
                                .unwrap()
                                .add(response.response.unwrap())
                        }
                    }
                }
                ViziaEvent::MenuTogglePressed => self.menu_visible = !self.menu_visible,
                ViziaEvent::TimerDurationChanged(t) => {
                    self.timer_duration = *t;
                }
                ViziaEvent::RefreshSites => {
                    self.sites = sites_to_pings(read_sites());
                    let _ = self.tx.send(TokioEvent::RefreshSites);
                    cx.emit(ViziaEvent::TimerReset);
                }
                ViziaEvent::AverageTogglePressed => {
                    for h in &mut self.history {
                        h.clear()
                    }
                    self.show_average = !self.show_average
                }
            }
        })
    }
}

/// Replacement for PingHistory.  Attempt #2
#[derive(Lens, Clone, PartialEq, Data)]
pub struct SiteAverage {
    pub name: String,
    pub sum: Duration,
    pub avg: String,
    pub len: u32,
}
impl SiteAverage {
    pub fn new(name: String) -> Self {
        SiteAverage {
            name,
            sum: Duration::ZERO,
            avg: String::new(),
            len: 0,
        }
    }

    pub fn add(&mut self, result: Duration) {
        self.sum += result;
        self.len += 1;
        self.avg = format!("{:.2?}", self.sum / self.len)
    }

    pub fn clear(&mut self) {
        self.len = 0;
        self.sum = Duration::ZERO;
        self.avg = String::new();
    }
}

/// Data structure for site name & ping response.  
#[derive(Lens, Clone, PartialEq, Data)]
pub struct PingResponse {
    pub name: String,
    pub response: Option<Duration>,
    pub is_err: bool,
}

/// Simple data structure for site name & ip address.
pub struct SiteAddress {
    pub name: String,
    pub addr: IpAddr,
}
