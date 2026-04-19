# PiZero2 Security System

Rust-based security monitor for Raspberry Pi Zero 2 WH. Subscribes to ONVIF events from a Tapo camera, grabs RTSP frames on motion/person detection, runs face recognition via Python sidecar, and sends Telegram notifications. Status shown on SH1106 OLED display via I2C.

## Architecture

```
Tapo camera
    │  ONVIF pull-point (SOAP/HTTP)
    ▼
main.rs  ──→  onvif.rs   (subscribe + pull events)
         ──→  camera.rs  (ffmpeg RTSP grab → face_recognizer.py)
         ──→  telegram.rs (send notification)
         ──→  display.rs  (SH1106 OLED over I2C)
```

## Hardware

| Component | Notes |
|-----------|-------|
| Raspberry Pi Zero 2 WH | target board |
| SH1106 128×64 OLED | I2C, wired to `/dev/i2c-1` |
| Tapo camera (C200/C210/etc.) | ONVIF + RTSP enabled |

**OLED wiring (I2C):**

| OLED pin | Pi Zero pin |
|----------|-------------|
| VCC | 3.3V (pin 1) |
| GND | GND (pin 6) |
| SCL | GPIO 3 / SCL (pin 5) |
| SDA | GPIO 2 / SDA (pin 3) |

## Prerequisites

### On your build machine (Linux/macOS/WSL)

1. **Rust toolchain**
   ```sh
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source $HOME/.cargo/env
   ```

2. **cross** – cross-compilation tool (builds `aarch64` binary on x86)
   ```sh
   cargo install cross
   ```
   `cross` requires Docker or Podman. Install [Docker Desktop](https://www.docker.com/products/docker-desktop/) or:
   ```sh
   # Ubuntu/Debian/WSL
   sudo apt install docker.io
   sudo usermod -aG docker $USER   # then log out and back in
   ```

3. **`aarch64-unknown-linux-gnu` target** (only needed for native cross without `cross` tool)
   ```sh
   rustup target add aarch64-unknown-linux-gnu
   ```

### On the Raspberry Pi Zero 2 WH

```sh
sudo apt update && sudo apt upgrade -y

# Enable I2C
sudo raspi-config
# → Interface Options → I2C → Enable → Finish → reboot

# Runtime dependencies
sudo apt install -y ffmpeg python3 python3-pip

# Face recognition dependencies
pip3 install opencv-python-headless face_recognition numpy
```

## Configuration

Edit constants at the top of [`src/main.rs`](src/main.rs):

```rust
const CAM_IP:    &str = "192.168.1.100";       // camera IP on your LAN
const CAM_USER:  &str = "admin";               // Tapo app username
const CAM_PASS:  &str = "YOUR_TAPO_PASSWORD";  // Tapo app password (not RTSP)
const RTSP_URL:  &str = "rtsp://admin:YOUR_RTSP_PASSWORD@192.168.1.100:554/stream2";

const BOT_TOKEN: &str = "YOUR_BOT_TOKEN";      // from @BotFather
const CHAT_ID:   &str = "YOUR_CHAT_ID";        // your Telegram chat/user ID
```

### Create a Telegram bot

1. Message [@BotFather](https://t.me/BotFather) → `/newbot`
2. Copy the token → `BOT_TOKEN`
3. Message [@userinfobot](https://t.me/userinfobot) → copy the id → `CHAT_ID`

### Find your RTSP password

In the Tapo app: **Camera settings → Advanced → RTSP**. Enable it and note the password (it is separate from your Tapo account password).

## Face recognizer

The binary calls `python3 /home/pi/face_recognizer.py recognize <image>` and expects JSON on stdout:

```json
{"faces": [{"name": "Honza", "confidence": 42.1}], "count": 1}
```

`face_recognizer.py` must support two subcommands:
- `train` – build model from labeled images
- `recognize <path>` – print JSON to stdout

Place labeled training images in `/home/pi/faces/<name>/` then run:

```sh
python3 /home/pi/face_recognizer.py train
```

Script path is configurable via `RECOGNIZER_PATH` in `main.rs`.

## Build

### Cross-compile for Pi Zero 2 (recommended)

```sh
make build
# equivalent: cross build --target aarch64-unknown-linux-gnu --release
```

Output: `target/aarch64-unknown-linux-gnu/release/PiZero2`

### Native build on Pi (slow, not recommended)

```sh
cargo build --release
```

### Lint & format check

```sh
make check
# equivalent: cargo clippy && cargo fmt --check
```

## Deploy & Run

**Copy binary to Pi:**
```sh
make deploy
# equivalent: scp target/aarch64-unknown-linux-gnu/release/PiZero2 pi@raspberrypi.local:~/
```

**Build + deploy in one step:**
```sh
make ship
```

**Run on Pi via SSH:**
```sh
make run
# equivalent: ssh pi@raspberrypi.local "./PiZero2"
```

**Run directly on Pi:**
```sh
./PiZero2
```

### Run as systemd service (auto-start on boot)

```sh
sudo nano /etc/systemd/system/pizero2.service
```

```ini
[Unit]
Description=PiZero2 Security System
After=network-online.target
Wants=network-online.target

[Service]
ExecStart=/home/pi/PiZero2
WorkingDirectory=/home/pi
Restart=on-failure
RestartSec=5
User=pi

[Install]
WantedBy=multi-user.target
```

```sh
sudo systemctl daemon-reload
sudo systemctl enable pizero2
sudo systemctl start pizero2
sudo journalctl -fu pizero2   # follow logs
```

## SSH setup (for make deploy/run)

If `pi@raspberrypi.local` doesn't resolve, either edit `PI_HOST` in `Makefile` to use the IP directly, or set up SSH key auth to avoid password prompts:

```sh
ssh-copy-id pi@raspberrypi.local
```

## Makefile targets

| Target | Action |
|--------|--------|
| `make build` | Cross-compile release binary |
| `make deploy` | scp binary to Pi |
| `make ship` | build + deploy |
| `make run` | ssh and run binary on Pi |
| `make check` | clippy + fmt check |

## Troubleshooting

**`Nelze otevřít I2C sběrnici (/dev/i2c-1)`**
I2C not enabled. Run `sudo raspi-config` → Interface Options → I2C → Enable, reboot.

**ONVIF subscribe fails / no events**
Enable ONVIF in Tapo app: Advanced → ONVIF. Verify camera IP and credentials.
ONVIF topic strings vary by firmware — run `GetEventProperties` to discover exact topics for your camera.

**ffmpeg timeout / camera unavailable**
Telegram notification still sent (without image). Verify RTSP URL and password.
Use `stream2` (sub-stream, 360p) not `stream1` — significantly faster grab on Pi Zero.

**Face recognition slow (~2–3s first call)**
Expected on Pi Zero 2. Python + OpenCV import overhead on each call.
For production: convert to a long-running Python daemon communicating via stdin/stdout or Unix socket.
