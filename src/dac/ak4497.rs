use crate::dac::common::{Akm44xxDac, FilterType, SampleRate};
use defmt::*;
use embassy_time::Timer;

impl Akm44xxDac {
    pub async fn initialize(&mut self, filter: u8, sound: u8) {
        self.filter_type = filter;
        self.sound_setting = sound;
        if self.pdn_pin.is_set_high() {
            self.pdn_pin.set_low();
        }
        Timer::after_millis(30).await;
        if self.pdn_pin.is_set_low() {
            self.pdn_pin.set_high();
        }

        info!("set up i2c ");
        Timer::after_millis(30).await;
        self.i2c_helper.write_register(0x0, 0b1001_0111).await;
        Timer::after_millis(30).await;

        self.filter(self.filter_type.into()).await;
        self.change_sound_setting(self.sound_setting).await;
        for i in 0..9 {
            let register = self.i2c_helper.read_register(i).await;
            info!("Register {:x} = {:b}", i, register)
        }
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
    pub async fn next_sound_setting(&mut self) -> u8 {
        if self.sound_setting > 3 {
            self.sound_setting = 0;
        }
        self.sound_setting += 1;
        self.change_sound_setting(self.sound_setting).await;
        self.reset().await;
        self.sound_setting
    }

    pub async fn filter(&mut self, typ: FilterType) {
        match typ {
            FilterType::Sharp => {
                self.i2c_helper.change_bit(5, 0, false).await;
                self.i2c_helper.change_bit(1, 5, false).await;
                self.i2c_helper.change_bit(2, 0, false).await;
            }
            FilterType::Slow => {
                self.i2c_helper.change_bit(5, 0, false).await;
                self.i2c_helper.change_bit(1, 5, false).await;
                self.i2c_helper.change_bit(2, 0, true).await;
            }
            FilterType::ShortDelaySharp => {
                self.i2c_helper.change_bit(5, 0, false).await;
                self.i2c_helper.change_bit(1, 5, true).await;
                self.i2c_helper.change_bit(2, 0, false).await;
            }
            FilterType::ShortDelaySlow => {
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
    pub async fn next_filter(&mut self) -> u8 {
        if self.filter_type > 4 {
            self.filter_type = 0;
        }
        self.filter_type += 1;
        self.filter(self.filter_type.into()).await;
        self.reset().await;
        self.filter_type
    }
    pub async fn dsd_pcm(&mut self, sample_rate: SampleRate) {
        if sample_rate.is_dsd() {
            // switch to DSD mode
            self.i2c_helper.change_bit(0, 0, false).await;
            self.i2c_helper.change_bit(2, 7, true).await;
            self.i2c_helper.change_bit(9, 2, true).await;
            self.i2c_helper.change_bit(0, 0, true).await;

            match sample_rate {
                SampleRate::Dsd64 => {
                    self.i2c_helper.change_bit(6, 0, false).await;
                    self.i2c_helper.change_bit(9, 0, false).await;
                }
                SampleRate::Dsd128 => {
                    self.i2c_helper.change_bit(6, 0, true).await;
                    self.i2c_helper.change_bit(9, 0, false).await;
                }
                SampleRate::Dsd256 => {
                    self.i2c_helper.change_bit(6, 0, false).await;
                    self.i2c_helper.change_bit(9, 0, true).await;
                }
                SampleRate::Dsd512 => {
                    self.i2c_helper.change_bit(6, 0, true).await;
                    self.i2c_helper.change_bit(9, 0, true).await;
                }
                _ => {}
            }
        } else {
            // switch to PCM mode
            self.i2c_helper.change_bit(0, 0, false).await;
            self.i2c_helper.change_bit(2, 7, false).await;
            self.i2c_helper.change_bit(9, 2, true).await;
            self.i2c_helper.change_bit(0, 0, true).await;
        }
    }
}
