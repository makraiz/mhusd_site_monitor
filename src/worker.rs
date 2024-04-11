use super::*;

/// Initates the runtime loop.  Must send ContextProxy first over mpsc channel, else panic!  
#[tokio::main] // Creates the runtime for us.
pub async fn tokio_main(rx: mpsc::Receiver<TokioEvent>) {
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
pub async fn ping(
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
