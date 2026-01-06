use crate::i2c_helper::I2CHelper;
use crate::{DacResources, IrqsI2c};
use embassy_rp::gpio::Level;
use embassy_rp::i2c::Config;
use embassy_rp::{gpio::Output, i2c};
use embassy_time::Timer;

pub struct Akm44xxDac {
    pub pdn_pin: Output<'static>,
    pub i2c_helper: I2CHelper,
    pub filter_type: u8,
    pub sound_setting: u8,
}
impl Akm44xxDac {
    pub fn new(resources: DacResources) -> Self {
        let cfg = Config::default();
        let i2c = i2c::I2c::new_async(
            resources.i2c,
            resources.pin15_i2c_scl,
            resources.pin14_i2c_sda,
            IrqsI2c,
            cfg,
        );
        let i2c_helper = I2CHelper::new(i2c).unwrap();
        Self {
            pdn_pin: Output::new(resources.pin2_dac_pdn, Level::High),
            i2c_helper,
            filter_type: FilterType::Sharp as u8,
            sound_setting: 0,
        }
    }

    pub async fn volume_up(&mut self) -> u8 {
        let current = self.i2c_helper.read_register(0x3).await;
        if let Some(new) = current.checked_add(3) {
            self.i2c_helper.write_register(0x3, new).await;
            self.i2c_helper.write_register(0x4, new).await;
            new
        } else {
            self.i2c_helper.write_register(0x3, 255).await;
            self.i2c_helper.write_register(0x4, 255).await;
            255
        }
    }

    pub async fn volume_down(&mut self) -> u8 {
        let current = self.i2c_helper.read_register(0x3).await;
        if let Some(new) = current.checked_sub(3) {
            self.i2c_helper.write_register(0x3, new).await;
            self.i2c_helper.write_register(0x4, new).await;
            new
        } else {
            self.i2c_helper.write_register(0x3, 0).await;
            self.i2c_helper.write_register(0x4, 0).await;
            0
        }
    }

    pub async fn set_volume(&mut self, vol: u8) {
        self.i2c_helper.write_register(0x3, vol).await;
        self.i2c_helper.write_register(0x4, vol).await;
    }

    pub async fn hi_load(&mut self, flag: bool) {
        self.i2c_helper.change_bit(8, 3, flag).await
    }

    pub async fn set_gain(&mut self, level: GainLevel) {
        match level {
            GainLevel::V25 => self.i2c_helper.write_register(7, 0b0000_0101),
            GainLevel::V28 => self.i2c_helper.write_register(7, 0b0000_0001),
            GainLevel::V375 => self.i2c_helper.write_register(7, 0b0000_1001),
        }
        .await;
    }

    pub async fn reset(&mut self) {
        self.i2c_helper.change_bit(0, 0, false).await;
        Timer::after_millis(50).await;
        self.i2c_helper.change_bit(0, 0, true).await;
    }
}

#[derive(Eq, PartialEq, PartialOrd, Clone, Copy, defmt::Format, Debug)]
pub enum SampleRate {
    Pcm32,
    Pcm441,
    Pcm48,
    Pcm882,
    Pcm96,
    Pcm1764,
    Pcm192,
    Pcm3528,
    Pcm384,
    Pcm7056,
    Pcm768,
    Pcm14112,
    Pcm1536,
    Dsd64,
    Dsd128,
    Dsd256,
    Dsd512,
    Dsd1024,
    Unknown,
}

impl SampleRate {
    pub fn to_str(self) -> (&'static str, &'static str, &'static str) {
        match self {
            SampleRate::Pcm32 => ("PCM", "32 kHz", "32 bit"),
            SampleRate::Pcm441 => ("PCM", "44.1 kHz", "32 bit"),
            SampleRate::Pcm48 => ("PCM", "48 kHz", "32 bit"),
            SampleRate::Pcm882 => ("PCM", "88.2 kHz", "32 bit"),
            SampleRate::Pcm96 => ("PCM", "96 kHz", "32 bit"),
            SampleRate::Pcm1764 => ("PCM", "176.4 kHz", "32 bit"),
            SampleRate::Pcm192 => ("PCM", "192 kHz", "32 bit"),
            SampleRate::Pcm3528 => ("PCM", "352.8 kHz", "32 bit"),
            SampleRate::Pcm384 => ("PCM", "384 kHz", "32 bit"),
            SampleRate::Pcm7056 => ("PCM", "705.6 kHz", "32 bit"),
            SampleRate::Pcm768 => ("PCM", "768 kHz", "32 bit"),
            SampleRate::Pcm14112 => ("PCM", "1411.2 kHz", "32 bit"),
            SampleRate::Pcm1536 => ("PCM", "1536 kHz", "32 bit"),
            SampleRate::Dsd64 => ("DSD", "DSD64", "1 bit"),
            SampleRate::Dsd128 => ("DSD", "DSD128", "1 bit"),
            SampleRate::Dsd256 => ("DSD", "DSD256", "1 bit"),
            SampleRate::Dsd512 => ("DSD", "DSD512", "1 bit"),
            SampleRate::Dsd1024 => ("DSD", "DSD1024", "1 bit"),
            SampleRate::Unknown => ("", "", ""),
        }
    }
    pub fn is_dsd(&self) -> bool {
        matches!(
            self,
            SampleRate::Dsd1024
                | SampleRate::Dsd512
                | SampleRate::Dsd256
                | SampleRate::Dsd128
                | SampleRate::Dsd64
        )
    }
}

pub enum FilterType {
    Sharp,
    Slow,
    SuperSlow,
    ShortDelaySharp,
    ShortDelaySlow,
}

impl FilterType {
    pub fn as_str(&self) -> &'static str {
        match self {
            FilterType::Sharp => "Sharp",
            FilterType::Slow => "Slow",
            FilterType::SuperSlow => "SSlow",
            FilterType::ShortDelaySharp => "ShD Sharp",
            FilterType::ShortDelaySlow => "ShD Slow",
        }
    }
}

impl From<u8> for FilterType {
    fn from(value: u8) -> Self {
        match value {
            0 => FilterType::Sharp,
            1 => FilterType::Slow,
            2 => FilterType::ShortDelaySharp,
            3 => FilterType::ShortDelaySlow,
            4 => FilterType::SuperSlow,
            _ => FilterType::Sharp,
        }
    }
}
pub enum GainLevel {
    V25,
    V28,
    V375,
}
