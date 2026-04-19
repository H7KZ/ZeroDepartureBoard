use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Baseline, Text},
};
use linux_embedded_hal::I2cdev;
use sh1106::{mode::GraphicsMode, Builder};

pub type Display = GraphicsMode<sh1106::interface::I2cInterface<I2cdev>>;

/// Inicializuje SH1106 OLED přes I2C sběrnici (/dev/i2c-1).
///
/// Zapni I2C na Pi:
///   `sudo raspi-config` → Interface Options → I2C → Enable
pub fn init() -> Display {
    let i2c = I2cdev::new("/dev/i2c-1").expect("Nelze otevřít I2C sběrnici (/dev/i2c-1)");

    let mut display: Display = Builder::new().connect_i2c(i2c).into();

    display.init().expect("Display init selhal");
    display.flush().expect("Display flush selhal");

    display
}

/// Vymaže displej a zobrazí text od levého horního rohu.
///
/// Dlouhé texty se zalomí automaticky (embedded-graphics to neřeší, takže
/// pokud potřebuješ více řádků, použij `show_lines`).
pub fn show_text(display: &mut Display, text: &str) {
    display.clear();

    let style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(BinaryColor::On)
        .build();

    Text::with_baseline(text, Point::new(0, 0), style, Baseline::Top)
        .draw(display)
        .expect("Draw selhal");

    display.flush().expect("Flush selhal");
}

/// Zobrazí více řádků textu (každý řádek = 12px výška s FONT_6X10).
#[allow(dead_code)]
pub fn show_lines(display: &mut Display, lines: &[&str]) {
    display.clear();

    let style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(BinaryColor::On)
        .build();

    for (i, line) in lines.iter().enumerate() {
        let y = i as i32 * 12; // řádková výška: font 10px + 2px mezera
        Text::with_baseline(line, Point::new(0, y), style, Baseline::Top)
            .draw(display)
            .expect("Draw selhal");
    }

    display.flush().expect("Flush selhal");
}
