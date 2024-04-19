use super::*;

pub fn vizia_main(tx: mpsc::Sender<TokioEvent>) {
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

        // Snapshot of current time.  Gets replaced pretty much immediately.
        let current_time = Local::now();

        // First round of pings.
        let _ = tx.send(TokioEvent::TimerElapsed);

        // Build sites list & history for GUI use.
        let sites = sites_to_pings(read_sites());
        let sites_history = get_history(&sites);

        // Create the data model for the GUI context.
        AppData {
            sites,
            timer,
            timer_count: 30,
            tx,
            menu_visible: false,
            timer_duration: 30,
            current_time,
            show_average: false,
            sites_history,
        }
        .build(cx);

        cx.start_timer(timer);
        cx.add_stylesheet(include_style!("style.css"))
            .expect("Failed to load style sheet!");

        // Window Layout
        HStack::new(cx, |cx| {
            left_side(cx);
            right_side(cx);
        })
        .class("windowBody");
    })
    .title("MHUSD Site Monitor")
    .run();
}

// Left side, site names and responses.
fn left_side(cx: &mut Context) -> Handle<VStack> {
    VStack::new(cx, |cx| {
        Binding::new(cx, AppData::show_average, |cx, show| {
            if show.get(cx) {
                List::new(cx, AppData::sites_history, |cx, _, site| {
                    HStack::new(cx, |cx| {
                        Label::new(cx, site.then(PingHistory::name)).class("siteName");
                        Label::new(cx, site.map(|h| h.avg())).class("siteResponse");
                    })
                    .col_between(Stretch(1.0))
                    .class("siteRow")
                    .toggle_class(
                        "siteRowError",
                        site.then(PingHistory::history).map(|h| {
                            for i in h {
                                if !i.is_err {
                                    return false;
                                }
                            }
                            true
                        }),
                    );
                });
            } else {
                List::new(cx, AppData::sites, |cx, _, site| {
                    HStack::new(cx, |cx| {
                        Label::new(cx, site.then(PingResponse::name)).class("siteName");
                        Label::new(
                            cx,
                            site.then(PingResponse::response).map(|r| {
                                if let Some(resp) = r {
                                    format!("{resp:.2?}")
                                } else {
                                    format!("Timeout!")
                                }
                            }),
                        )
                        .class("siteResponse");
                    })
                    .col_between(Stretch(1.0))
                    .class("siteRow")
                    .toggle_class("siteRowError", site.then(PingResponse::is_err));
                });
            }
        }); // End of show_average Binding
        Element::new(cx); //Exists to take up space
        Label::new(
            cx,
            AppData::current_time.map(|t| format!("Last Update: {}", t.format("%r"))),
        )
        .class("timeStamp");
    })
    .class("leftPane")
}

// Right side, timer countdown and controls.
fn right_side(cx: &mut Context) -> Handle<VStack> {
    VStack::new(cx, |cx| {
        HStack::new(cx, |cx| {
            Element::new(cx); // Exists to take up space.
            Label::new(cx, "Show controls: ").class("menuToggleLabel");
            Switch::new(cx, AppData::menu_visible)
                .on_toggle(|cx| cx.emit(ViziaEvent::MenuTogglePressed))
                .class("menuToggleButton");
        })
        .class("menuButtonBar");
        HStack::new(cx, |cx| {
            Binding::new(cx, AppData::menu_visible, |cx, show| {
                if show.get(cx) {
                    VStack::new(cx, |cx| {
                        HStack::new(cx, |cx| {
                            // Average results toggle
                            Element::new(cx); // Exists to take up space.
                            Label::new(cx, "Average results: ").class("menuToggleLabel");
                            Switch::new(cx, AppData::show_average)
                                .on_toggle(|cx| cx.emit(ViziaEvent::AverageTogglePressed))
                                .class("menuInput");
                        })
                        .class("menuButtonBar");

                        HStack::new(cx, |cx| {
                            // Timer interval control
                            Element::new(cx); // Exists to take up space.
                            Label::new(cx, "Refresh interval: ").class("menuInputLabel");
                            Textbox::new(cx, AppData::timer_duration)
                                .on_submit(|ex, text, _| {
                                    ex.emit(ViziaEvent::TimerDurationChanged(text))
                                })
                                .class("menuInput");
                        })
                        .class("menuInputRow");

                        HStack::new(cx, |cx| {
                            // Refresh now button
                            Element::new(cx); // Exists to take up space.
                            Button::new(cx, |cx| Label::new(cx, "Refresh now"))
                                .on_press(|ex| ex.emit(ViziaEvent::TimerReset))
                                .class("menuInput");
                        })
                        .class("menuInputRow");

                        HStack::new(cx, |cx| {
                            // Reload sites button
                            Element::new(cx); // Exists to take up space.
                            Button::new(cx, |cx| Label::new(cx, "Reload sites.json"))
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
            Label::new(cx, "Next refresh in:").class("timerLabel");
            Label::new(cx, AppData::timer_count).class("timerCount");
        })
        .class("timerPane")
        .col_between(Stretch(1.0));
    })
    .class("rightPane")
    .row_between(Stretch(1.0))
}
