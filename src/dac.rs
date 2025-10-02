use crate::{i2c_helper, DacResources, IrqsI2c};
use defmt::*;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::i2c::{self, Config};
use embassy_time::Timer;
pub struct Dac {
    pdn_pin: Output<'static>,
    i2c_helper: i2c_helper::I2CHelper,
    filter_type: u8,
    sound_setting: u8,
    dp_mode: u8,
}
impl Dac {
    pub fn new(resources: DacResources) -> Self {
        let cfg = Config::default();
        let i2c = i2c::I2c::new_async(
            resources.i2c,
            resources.pin15_i2c_scl,
            resources.pin14_i2c_sda,
            IrqsI2c,
            cfg,
        );
        let i2c_helper = i2c_helper::I2CHelper::new(i2c).unwrap();
        Self {
            pdn_pin: Output::new(resources.pin2_dac_pdc, Level::High),
            i2c_helper,
            filter_type: FilterType::SharpRollOff as u8,
            sound_setting: 4,
            dp_mode: 0,
        }
    }
    pub async fn initialize(&mut self) {
        // initialize dac
        if self.pdn_pin.is_set_high() {
            self.pdn_pin.set_low();
        }
        Timer::after_millis(30).await;
        if self.pdn_pin.is_set_low() {
            self.pdn_pin.set_high();
        }
        Timer::after_millis(30).await;
        info!("set up i2c ");
        Timer::after_millis(30).await;
        self.i2c_helper.write_register(0x0, 0b1001_0111).await;
        Timer::after_millis(30).await;
        // self.i2c_helper.write_register(0x1, 0b0000_0010);
        // Timer::after_millis(30);

        self.filter(self.filter_type.into()).await;
        self.change_sound_setting(self.sound_setting).await;
        self.dsd_pcm(false, None).await;
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

    pub async fn change_sound_setting(&mut self, setting_no: u8) {
        match setting_no {
            1 => {
                self.i2c_helper.change_bit(8, 0, false).await;
                self.i2c_helper.change_bit(8, 1, false).await;
                self.i2c_helper.change_bit(8, 2, false).await;
            }
            2 => {
                self.i2c_helper.change_bit(8, 0, true).await;
                self.i2c_helper.change_bit(8, 1, false).await;
                self.i2c_helper.change_bit(8, 2, false).await;
            }
            3 => {
                self.i2c_helper.change_bit(8, 0, false).await;
                self.i2c_helper.change_bit(8, 1, true).await;
                self.i2c_helper.change_bit(8, 2, false).await;
            }
            4 => {
                self.i2c_helper.change_bit(8, 0, true).await;
                self.i2c_helper.change_bit(8, 1, true).await;
                self.i2c_helper.change_bit(8, 2, false).await;
            }
            _ => {
                self.i2c_helper.change_bit(8, 0, false).await;
                self.i2c_helper.change_bit(8, 1, false).await;
                self.i2c_helper.change_bit(8, 2, true).await;
            }
        }
    }
    pub async fn next_sound_setting(&mut self) {
        self.sound_setting += 1;
        if self.sound_setting > 5 {
            self.sound_setting = 0;
        }
        self.change_sound_setting(self.sound_setting).await;
        self.reset().await;
    }

    pub async fn filter(&mut self, typ: FilterType) {
        match typ {
            FilterType::SharpRollOff => {
                self.i2c_helper.change_bit(5, 0, false).await;
                self.i2c_helper.change_bit(1, 5, false).await;
                self.i2c_helper.change_bit(2, 0, false).await;
            }
            FilterType::SlowRollOff => {
                self.i2c_helper.change_bit(5, 0, false).await;
                self.i2c_helper.change_bit(1, 5, false).await;
                self.i2c_helper.change_bit(2, 0, true).await;
            }
            FilterType::ShortDelaySharpRollOff => {
                self.i2c_helper.change_bit(5, 0, false).await;
                self.i2c_helper.change_bit(1, 5, true).await;
                self.i2c_helper.change_bit(2, 0, false).await;
            }
            FilterType::ShortDelaySlowRollOff => {
                self.i2c_helper.change_bit(5, 0, false).await;
                self.i2c_helper.change_bit(1, 5, true).await;
                self.i2c_helper.change_bit(2, 0, true).await;
            }
            FilterType::SuperSlow => {
                self.i2c_helper.change_bit(5, 0, true).await;
                self.i2c_helper.change_bit(1, 5, false).await;
                self.i2c_helper.change_bit(2, 0, false).await;
            }
        };
    }
    pub async fn next_filter(&mut self) {
        self.filter_type += 1;
        if self.filter_type > 4 {
            self.filter_type = 0;
        }
        self.filter(self.filter_type.into()).await;
        self.reset().await;
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
    async fn reset(&mut self) {
        self.i2c_helper.change_bit(0, 0, false).await;
        Timer::after_millis(50).await;
        self.i2c_helper.change_bit(0, 0, true).await;
    }
    pub async fn dsd_pcm(&mut self, dsd: bool, sampling_speed: Option<DSDSamplingSpeed>) {
        if dsd {
            // switch to DSD mode
            self.i2c_helper.change_bit(0, 0, false).await;
            self.i2c_helper.change_bit(2, 7, true).await;
            self.i2c_helper.change_bit(9, 2, true).await;
            self.i2c_helper.change_bit(0, 0, true).await;

            if let Some(speed) = sampling_speed {
                match speed {
                    DSDSamplingSpeed::DSD64 => {
                        self.i2c_helper.change_bit(6, 0, false).await;
                        self.i2c_helper.change_bit(9, 0, false).await;
                    }
                    DSDSamplingSpeed::DSD128 => {
                        self.i2c_helper.change_bit(6, 0, true).await;
                        self.i2c_helper.change_bit(9, 0, false).await;
                    }
                    DSDSamplingSpeed::DSD256 => {
                        self.i2c_helper.change_bit(6, 0, false).await;
                        self.i2c_helper.change_bit(9, 0, true).await;
                    }
                    DSDSamplingSpeed::DSD512 => {
                        self.i2c_helper.change_bit(6, 0, true).await;
                        self.i2c_helper.change_bit(9, 0, true).await;
                    }
                }
            }
        } else {
            // switch to PCM mode
            self.i2c_helper.change_bit(0, 0, false).await;
            self.i2c_helper.change_bit(2, 7, false).await;
            self.i2c_helper.change_bit(9, 2, true).await;
            self.i2c_helper.change_bit(0, 0, true).await;

            // self.i2c_helper.change_bit(6, 0, false).await;
            // self.i2c_helper.change_bit(9, 0, false).await;
        }
    }
    pub async fn toggle_dsd_pcm(&mut self) {
        self.dp_mode += 1;
        match self.dp_mode {
            1 => {
                self.dsd_pcm(true, Some(DSDSamplingSpeed::DSD64)).await;
            }
            2 => {
                self.dsd_pcm(true, Some(DSDSamplingSpeed::DSD128)).await;
            }
            3 => {
                self.dsd_pcm(true, Some(DSDSamplingSpeed::DSD256)).await;
            }
            4 => {
                self.dsd_pcm(true, Some(DSDSamplingSpeed::DSD512)).await;
            }
            5 => {
                self.dsd_pcm(false, None).await;
                self.dp_mode = 0;
            }
            _ => {
                self.dp_mode = 0;
            }
        }
    }
    pub async fn toggle_dsd_dclk_polarity(&mut self) {
        self.i2c_helper.toggle_bit(2, 4).await;
    }
    pub async fn toggle_dsd_dcks_clock(&mut self) {
        self.i2c_helper.toggle_bit(2, 5).await;
    }

    pub async fn toggle_dsd_cutoff_freq_filter(&mut self) {
        self.i2c_helper.toggle_bit(9, 1).await;
    }
}

pub enum FilterType {
    SharpRollOff,
    SlowRollOff,
    ShortDelaySharpRollOff,
    ShortDelaySlowRollOff,
    SuperSlow,
}
impl From<u8> for FilterType {
    fn from(value: u8) -> Self {
        match value {
            0 => FilterType::SharpRollOff,
            1 => FilterType::SlowRollOff,
            2 => FilterType::ShortDelaySharpRollOff,
            3 => FilterType::ShortDelaySlowRollOff,
            _ => FilterType::SuperSlow,
        }
    }
}

pub enum GainLevel {
    V25,
    V28,
    V375,
}

pub enum DSDSamplingSpeed {
    DSD64,
    DSD128,
    DSD256,
    DSD512,
}
