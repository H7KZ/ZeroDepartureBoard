mod camera;
mod display;
mod motion;
mod onvif;
mod telegram;

use onvif::{CameraEvent, EventType, OnvifClient};
use std::time::{Duration, Instant};

// Kamera
const CAM_IP: &str = "192.168.1.100";
const CAM_USER: &str = "admin";
const CAM_PASS: &str = "TVůJ_TAPO_HESLO"; // Tapo heslo (ne RTSP)
const RTSP_URL: &str = "rtsp://admin:TVůJ_RTSP_HESLO@192.168.1.100:554/stream2";
const FRAME_PATH: &str = "/tmp/motion_frame.jpg";

// Telegram
const BOT_TOKEN: &str = "TVůJ_BOT_TOKEN";
const CHAT_ID: &str = "TVůJ_CHAT_ID";

// Logika
/// Cooldown mezi notifikacemi (sekundy) – zabrání spamu při opakovaném triggeru
const COOLDOWN_SECS: u64 = 30;

/// Cesta k Python face recognizeru (viz README)
const RECOGNIZER_PATH: &str = "/home/pi/face_recognizer.py";

#[tokio::main]
async fn main() {
    println!("=== PiZero2 Security System ===");

    let mut display = display::init();
    let mut onvif = OnvifClient::new(CAM_IP, CAM_USER, CAM_PASS);

    display::show_text(&mut display, "Připojuji...");

    // Subscribe s nekonečným retry
    subscribe_with_retry(&mut onvif, &mut display).await;

    display::show_text(&mut display, "Čekám na pohyb...");
    println!("ONVIF připojen, čekám na eventy...");

    let mut last_trigger: Option<Instant> = None;

    loop {
        match onvif.pull_events().await {
            Ok(events) => {
                for event in events {
                    println!("[EVENT] {:?}", event);
                    handle_event(event, &mut display, &mut last_trigger).await;
                }
            }
            Err(e) => {
                eprintln!("[ONVIF] Pull selhal: {} – resubscribe", e);
                display::show_text(&mut display, "Rekonek...");
                tokio::time::sleep(Duration::from_secs(3)).await;
                subscribe_with_retry(&mut onvif, &mut display).await;
                display::show_text(&mut display, "Čekám na pohyb...");
            }
        }
        // PullMessages drží spojení max. 5 s → pokud přijdou eventy dříve,
        // smyčka se okamžitě opakuje bez zbytečného sleep()
    }
}

/// Zpracuje jeden event z kamery
async fn handle_event(
    event: CameraEvent,
    display: &mut display::Display,
    last_trigger: &mut Option<Instant>,
) {
    match event.event_type {
        EventType::MotionStopped => {
            display::show_text(display, "Čekám na pohyb...");
        }

        EventType::PersonDetected | EventType::MotionDetected => {
            // Cooldown check
            if let Some(t) = last_trigger {
                if t.elapsed().as_secs() < COOLDOWN_SECS {
                    println!("[COOLDOWN] přeskočeno ({:?})", event.event_type);
                    return;
                }
            }
            *last_trigger = Some(Instant::now());

            let is_person = matches!(event.event_type, EventType::PersonDetected);
            trigger_detection(display, is_person).await;
        }

        EventType::Unknown(ref topic) => {
            println!("[ONVIF] Neznámý topic: {}", topic);
        }
    }
}

/// Grab frame → face recognition → Telegram + display
async fn trigger_detection(display: &mut display::Display, is_person: bool) {
    let label = if is_person { "Osoba!" } else { "Pohyb!" };
    display::show_text(display, label);
    println!("[DETECT] {}", label);

    // 1. Grab frame z RTSP
    match camera::grab_frame(RTSP_URL, FRAME_PATH) {
        Err(e) => {
            let msg = format!("🔔 {} (kamera nedostupná)", label);
            eprintln!("[CAMERA] {}", e);
            telegram::send(BOT_TOKEN, CHAT_ID, &msg).await;
            display::show_text(display, "Kamera chyba");
        }
        Ok(_) => {
            // 2. Face recognition přes Python sidecar
            let recognition = camera::run_face_recognition(RECOGNIZER_PATH, FRAME_PATH);
            let message = build_notification(recognition.as_deref(), is_person);

            // 3. Notifikace
            display::show_text(display, &message);
            telegram::send(BOT_TOKEN, CHAT_ID, &message).await;
        }
    }
}

/// Sestaví text notifikace z JSON výstupu recognizeru
fn build_notification(recognition_json: Option<&str>, is_person: bool) -> String {
    if let Some(json) = recognition_json {
        // Pokud recognizer vrátí jméno (ne "unknown"), použij ho
        if let Some(name) = extract_first_name(json) {
            return format!("🏠 Přišel: {}", name);
        }
    }

    if is_person {
        "👤 Neznámá osoba u dveří".to_string()
    } else {
        "🔔 Pohyb detekován".to_string()
    }
}

/// Vytáhne první rozpoznané jméno z JSON výstupu face_recognizer.py
/// Příklad JSON: {"faces": [{"name": "Honza", "confidence": 42.1}], "count": 1}
fn extract_first_name(json: &str) -> Option<String> {
    let name = json.split("\"name\":\"").nth(1)?.split('"').next()?;

    if name == "unknown" {
        None
    } else {
        Some(name.to_string())
    }
}

/// Připojuje se dokud se to nepovede
async fn subscribe_with_retry(onvif: &mut OnvifClient, display: &mut display::Display) {
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        println!("[ONVIF] Subscribe pokus #{}", attempt);
        match onvif.subscribe().await {
            Ok(_) => {
                println!("[ONVIF] Subscribed OK");
                return;
            }
            Err(e) => {
                eprintln!("[ONVIF] Chyba: {}", e);
                let msg = format!("Retry #{}", attempt);
                display::show_text(display, &msg);
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}
