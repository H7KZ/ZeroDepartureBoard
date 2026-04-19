use reqwest::Client;
use std::time::Duration;

/// Pošle textovou zprávu přes Telegram Bot API.
///
/// Tichý fail – pokud se odeslání nepovede, jen se zaloguje chyba.
/// Pro kritické notifikace přidej retry logiku (viz `send_with_retry`).
pub async fn send(token: &str, chat_id: &str, text: &str) {
    if let Err(e) = send_inner(token, chat_id, text).await {
        eprintln!("[TELEGRAM] Odeslání selhalo: {}", e);
    }
}

/// Varianta s retry – pokus max. 3×, s exponenciálním backoffem.
#[allow(dead_code)]
pub async fn send_with_retry(token: &str, chat_id: &str, text: &str) {
    for attempt in 1..=3u32 {
        match send_inner(token, chat_id, text).await {
            Ok(_) => return,
            Err(e) => {
                eprintln!("[TELEGRAM] Pokus #{} selhal: {}", attempt, e);
                if attempt < 3 {
                    let backoff = Duration::from_secs(2u64.pow(attempt));
                    tokio::time::sleep(backoff).await;
                }
            }
        }
    }
    eprintln!("[TELEGRAM] Všechny pokusy selhaly, zpráva nedoručena");
}

// Interní

async fn send_inner(token: &str, chat_id: &str, text: &str) -> Result<(), String> {
    let url = format!("https://api.telegram.org/bot{}/sendMessage", token);

    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .post(&url)
        .json(&serde_json::json!({
            "chat_id":    chat_id,
            "text":       text,
            "parse_mode": "HTML",   // umožní <b>tučné</b> texty v notifikacích
        }))
        .send()
        .await
        .map_err(|e| format!("HTTP chyba: {}", e))?;

    if response.status().is_success() {
        println!("[TELEGRAM] Odesláno: {}", text);
        Ok(())
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Err(format!("API vrátilo {}: {}", status, body))
    }
}
