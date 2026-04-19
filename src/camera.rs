use std::process::Command;
use std::time::Duration;

// RTSP frame grab

/// Vytáhne jeden JPEG frame z RTSP streamu přes ffmpeg.
///
/// Vyžaduje `ffmpeg` nainstalovaný v systému:
///   `sudo apt install ffmpeg -y`
///
/// Používej `stream2` (sub-stream 360p) – výrazně rychlejší grab na Pi Zero.
pub fn grab_frame(rtsp_url: &str, output_path: &str) -> Result<(), String> {
    // ffmpeg timeout: pokud kamera nereaguje, nezasekneme se navždy
    let timeout_us = (Duration::from_secs(8).as_micros()).to_string();

    let status = Command::new("ffmpeg")
        .args([
            "-loglevel",
            "error", // nezahltit stdout
            "-rtsp_transport",
            "tcp", // TCP je stabilnější než UDP přes WiFi
            "-timeout",
            &timeout_us, // socket read timeout v mikrosekundách
            "-i",
            rtsp_url,
            "-frames:v",
            "1", // jen 1 frame
            "-q:v",
            "3",  // kvalita JPEG (2=nejlepší, 5=dostatečné)
            "-y", // přepiš existující soubor
            output_path,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("ffmpeg spawn selhal: {}", e))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "ffmpeg vrátil chybu (exit code: {:?})",
            status.code()
        ))
    }
}

// Face recognition (Python sidecar)

/// Spustí Python face recognizer a vrátí jeho JSON výstup jako String.
///
/// # Argumenty
/// - `recognizer_path`: cesta k `face_recognizer.py`
/// - `image_path`: cesta k JPEG framu (výstup `grab_frame`)
///
/// # Návratová hodnota
/// - `Some(json_string)` pokud recognizer proběhl a vrátil výstup
/// - `None` pokud subprocess selhal nebo vrátil prázdný výstup
///
/// # Poznámka k výkonu
/// Na Pi Zero 2 WH trvá první spuštění Pythonu + import OpenCV ~2-3 s.
/// Pro produkci zvažte long-running Python daemon komunikující přes stdin/stdout
/// nebo Unix socket (viz README).
pub fn run_face_recognition(recognizer_path: &str, image_path: &str) -> Option<String> {
    let output = Command::new("python3")
        .args([recognizer_path, "recognize", image_path])
        .output()
        .map_err(|e| eprintln!("[CAMERA] Python spawn selhal: {}", e))
        .ok()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("[CAMERA] Recognizer stderr: {}", stderr);
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if stdout.is_empty() {
        eprintln!("[CAMERA] Recognizer vrátil prázdný výstup");
        None
    } else {
        Some(stdout)
    }
}

// Training helper (volitelné – spusť z CLI nebo přes Telegram příkaz)

/// Spustí trénování modelu (`python3 face_recognizer.py train`).
/// Vrátí JSON výstup confirming přidané osoby.
#[allow(dead_code)]
pub fn train_model(recognizer_path: &str) -> Result<String, String> {
    let output = Command::new("python3")
        .args([recognizer_path, "train"])
        .output()
        .map_err(|e| format!("Python spawn selhal: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}
