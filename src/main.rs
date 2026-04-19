mod api;
mod board;
mod config;
mod display;
mod motion;

use std::time::{Duration, Instant};

enum State {
    Idle,
    Active {
        last_motion: Instant,
        /// None = fetch immediately on next tick.
        last_fetch: Option<Instant>,
    },
}

#[tokio::main]
async fn main() {
    println!("=== ZeroDepartureBoard ===");
    println!("Backend: {}", config::BACKEND_URL);
    println!("Stop:    {}", config::STOP_NAME);

    let client = reqwest::Client::new();
    let mut display = display::init();
    let pir = motion::init_pin(config::PIR_GPIO_PIN);

    display::show_status(&mut display, "Cekam na pohyb...");

    let mut state = State::Idle;

    loop {
        let motion = motion::is_detected(&pir);

        state = match state {
            State::Idle => {
                if motion {
                    println!("[MOTION] Detected — waking display");
                    State::Active {
                        last_motion: Instant::now(),
                        last_fetch: None,
                    }
                } else {
                    State::Idle
                }
            }

            State::Active { mut last_motion, last_fetch } => {
                if motion {
                    last_motion = Instant::now();
                }

                if last_motion.elapsed().as_secs() >= config::IDLE_TIMEOUT_SECS {
                    println!("[IDLE] No motion for {}s — sleeping display", config::IDLE_TIMEOUT_SECS);
                    display::sleep(&mut display);
                    State::Idle
                } else {
                    let should_fetch = last_fetch
                        .map(|t| t.elapsed().as_secs() >= config::POLL_INTERVAL_SECS)
                        .unwrap_or(true);

                    let next_fetch = if should_fetch {
                        match api::fetch(&client).await {
                            Ok(data) => {
                                let stop = data.stop_name.as_deref().unwrap_or(config::STOP_NAME);
                                let header = board::header(stop);
                                let rows = board::render(&data.departures, config::MAX_DEPARTURES);
                                display::render_board(&mut display, &header, &rows);
                                println!("[API] OK — {} departures for '{}'", rows.len(), stop);
                            }
                            Err(e) => {
                                eprintln!("[API] Error: {e}");
                                display::show_status(&mut display, "Chyba spojeni");
                            }
                        }
                        Some(Instant::now())
                    } else {
                        last_fetch
                    };

                    State::Active {
                        last_motion,
                        last_fetch: next_fetch,
                    }
                }
            }
        };

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
