PI_HOST = pi@raspberrypi.local
TARGET = aarch64-unknown-linux-gnu
BIN = PiZero2

.PHONY: build deploy ship check run

build:
	cross build --target $(TARGET) --release

deploy:
	scp target/$(TARGET)/release/$(BIN) $(PI_HOST):~/

ship: build deploy

check:
	cargo clippy
	cargo fmt --check

run:
	ssh $(PI_HOST) "./$(BIN)"
