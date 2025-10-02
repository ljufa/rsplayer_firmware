use defmt::debug;
use embassy_rp::i2c::{self, Error};
use embassy_time::Timer;
use embedded_hal_1::i2c::I2c;

use crate::POWER_ON;

pub struct I2CHelper {
    i2c: i2c::I2c<'static, embassy_rp::peripherals::I2C1, i2c::Async>,
    addr: u8,
}

impl I2CHelper {
    pub fn new(
        i2c: i2c::I2c<'static, embassy_rp::peripherals::I2C1, i2c::Async>,
    ) -> Result<Self, Error> {
        Ok(I2CHelper { i2c, addr: 0x13 })
    }

    pub(crate) async fn write_register(&mut self, reg_addr: u8, value: u8) {
        if !POWER_ON.load(core::sync::atomic::Ordering::Relaxed) {
            return;
        }
        debug!("I2C write reg_addr:{}, value: {}", reg_addr, value);
        self.i2c.write(self.addr, &[reg_addr, value]).unwrap();
    }

    pub async fn read_register(&mut self, reg_addr: u8) -> u8 {
        if !POWER_ON.load(core::sync::atomic::Ordering::Relaxed) {
            return 0;
        }

        let mut data = [0u8; 1];
        self.i2c
            .write_read(self.addr, &[reg_addr], &mut data)
            .unwrap();
        debug!("I2C read reg_addr:{}, value: {}", reg_addr, data[0]);
        data[0]
    }
    pub async fn change_bit(&mut self, reg_addr: u8, bit_pos: u8, value: bool) {
        if !POWER_ON.load(core::sync::atomic::Ordering::Relaxed) {
            return;
        }

        let mut data = self.read_register(reg_addr).await;
        if value {
            data |= 1 << bit_pos;
        } else {
            data &= !(1 << bit_pos);
        }
        self.write_register(reg_addr, data).await;
        Timer::after_millis(30).await;
    }

    pub async fn toggle_bit(&mut self, reg_addr: u8, bit_pos: u8) {
        if !POWER_ON.load(core::sync::atomic::Ordering::Relaxed) {
            return;
        }

        let mut data = self.read_register(reg_addr).await;
        data ^= 1 << bit_pos;
        self.write_register(reg_addr, data).await;
    }
}
