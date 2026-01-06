use core::fmt::Write;
use defmt::debug;
use embassy_rp::gpio::Level;

use embedded_hal_bus::spi::ExclusiveDevice;
use heapless::String;
use u8g2_fonts::{
    types::{FontColor, VerticalPosition},
    FontRenderer,
};

use crate::DisplayResources;

use {defmt_rtt as _, panic_probe as _};

use embassy_rp::gpio::Output;
use embassy_rp::spi;
use embassy_time::{Delay, Instant};
use embedded_graphics::{
    pixelcolor::BinaryColor,
    prelude::{Point, *},
    primitives::Rectangle,
};

struct DummyCs;
impl embedded_hal_1::digital::OutputPin for DummyCs {
    fn set_low(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn set_high(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}
impl embedded_hal_1::digital::ErrorType for DummyCs {
    type Error = core::convert::Infallible;
}

type SpiDeviceDisp = ExclusiveDevice<
    spi::Spi<'static, embassy_rp::peripherals::SPI0, spi::Async>,
    DummyCs,
    Delay,
>;

type Display = st7920::ST7920<SpiDeviceDisp, Output<'static>, Output<'static>>;

pub struct OledDisplay {
    display: Display,
    blk_pin: Output<'static>,
    font_huge: FontRenderer,
    font_medium: FontRenderer,
    font_small: FontRenderer,
    pub last_update: Instant,
}

impl OledDisplay {
    pub fn new(disp_res: DisplayResources) -> Self {
        // let cs_pin = Output::new(disp_res.pin5_dummy_cs, Level::Low);
        let rst_pin = Output::new(disp_res.pin20_spi_rst, Level::Low);

        let mut config = spi::Config::default();
        config.frequency = 800_000;
        let spi_disp = spi::Spi::new_txonly(
            disp_res.spi0,
            disp_res.pin18_spi_sck,
            disp_res.pin19_spi_tx,
            disp_res.dmach3,
            config,
        );
        let spi_dev = ExclusiveDevice::new(spi_disp, DummyCs, Delay);
        let mut disp = Display::new(spi_dev, rst_pin, None::<Output>, false);
        disp.init(&mut Delay).expect("could not init display");
        disp.clear(&mut Delay).expect("could not clear display");

        Self {
            display: disp,
            blk_pin: Output::new(disp_res.pin22_blk_gnd, Level::Low),
            font_huge: FontRenderer::new::<u8g2_fonts::fonts::u8g2_font_fub25_tf>(),
            font_medium: FontRenderer::new::<u8g2_fonts::fonts::u8g2_font_helvB12_te>(),
            font_small: FontRenderer::new::<u8g2_fonts::fonts::u8g2_font_helvB08_te>(),
            last_update: Instant::now(),
        }
    }

    pub fn turn_off_backlight(&mut self) {
        self.blk_pin.set_low();
    }
    pub fn turn_on_backlight(&mut self) {
        self.blk_pin.set_high();
        self.last_update = Instant::now();
    }
    pub fn draw_powered_off(&mut self) {
        self.display.clear(&mut Delay).unwrap();
        self.font_huge
            .render_aligned(
                "Off",
                self.display.bounding_box().center(),
                VerticalPosition::Top,
                u8g2_fonts::types::HorizontalAlignment::Center,
                FontColor::Transparent(BinaryColor::On),
                &mut self.display,
            )
            .unwrap();
        self.display.flush(&mut Delay).unwrap();
    }
    pub fn clear(&mut self) {
        self.display.clear(&mut Delay).unwrap();
    }

    pub fn draw_powering_on(&mut self) {
        self.display.clear(&mut Delay).unwrap();
        self.turn_on_backlight();
        self.font_huge
            .render_aligned(
                "Starting",
                self.display.bounding_box().center(),
                VerticalPosition::Top,
                u8g2_fonts::types::HorizontalAlignment::Center,
                FontColor::Transparent(BinaryColor::On),
                &mut self.display,
            )
            .unwrap();
        self.display.flush(&mut Delay).unwrap();
    }

    pub fn draw_header_status(&mut self, input: &str, _filter: &str) {
        self.turn_on_backlight();
        let area = Rectangle::new(
            Point { x: 60, y: 36 },
            Size {
                width: 60,
                height: 20,
            },
        );
        let mut buff = String::<32>::new();
        write!(&mut buff, "IN: {}", input).unwrap();
        self.display.fill_solid(&area, BinaryColor::Off).unwrap();
        self.font_small
            .render(
                buff.as_str(),
                area.top_left,
                VerticalPosition::Top,
                FontColor::Transparent(BinaryColor::On),
                &mut self.display,
            )
            .unwrap();
        self.flush_region(&area);
    }

    pub fn draw_volume(&mut self, volume: u8) {
        self.turn_on_backlight();
        let mut buff = String::<32>::new();
        let v = volume as f32;
        let db = (v - 255.00) * 0.5;
        write!(&mut buff, "{: >6}", db).unwrap();

        let area = Rectangle::new(
            Point { x: 6, y: 5 },
            Size {
                width: 128 - 10,
                height: 29,
            },
        );
        self.display.fill_solid(&area, BinaryColor::Off).unwrap();
        self.font_small
            .render(
                "Vol",
                area.top_left,
                VerticalPosition::Top,
                FontColor::Transparent(BinaryColor::On),
                &mut self.display,
            )
            .unwrap();
        self.font_medium
            .render(
                "dB",
                Point {
                    x: area.top_left.x,
                    y: area.top_left.y + 12,
                },
                VerticalPosition::Top,
                FontColor::Transparent(BinaryColor::On),
                &mut self.display,
            )
            .unwrap();

        self.font_huge
            .render(
                buff.as_str(),
                Point {
                    x: area.top_left.x + 23,
                    y: area.top_left.y,
                },
                VerticalPosition::Top,
                FontColor::Transparent(BinaryColor::On),
                &mut self.display,
            )
            .unwrap();
        debug!("end");
        self.flush_region(&area);
    }

    fn flush_region(&mut self, area: &Rectangle) {
        self.display
            .flush_region(
                area.top_left.x as u8,
                area.top_left.y as u8,
                area.size.width as u8,
                area.size.height as u8,
                &mut Delay,
            )
            .unwrap();
    }

    pub fn tick(&mut self) {}
    pub fn draw_background(&mut self) {}
    pub fn draw_layout_lines(&mut self) {}
    pub fn draw_track_info(&mut self, _title: &str, _artist: &str, _album: &str) {}
    pub fn draw_progress_bar(&mut self, _curr_time: &str, _total_time: &str, _progress: f32) {}
    pub fn draw_footer(&mut self, _format: &str, _freq: &str, _bit_depth: &str) {}
}
