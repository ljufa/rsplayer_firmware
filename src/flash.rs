use embassy_rp::{
    flash::{Async, Flash, ERASE_SIZE},
    peripherals::FLASH,
};

use crate::FlashResources;

const FLASH_SIZE: usize = 2 * 1024 * 1024;
const ADDR_OFFSET: u32 = 0x100000;

const VOLUME_OFFSET: u32 = 0x00;
const INPUT_OFFSET: u32 = 0x1000;

pub struct Storage {
    flash: Flash<'static, FLASH, Async, FLASH_SIZE>,
}
impl Storage {
    pub fn new(res: FlashResources) -> Self {
        Storage {
            flash: embassy_rp::flash::Flash::<_, Async, FLASH_SIZE>::new(res.flash, res.dma_ch4),
        }
    }
    pub fn save_volume(&mut self, volume: u8) {
        self.write_u8(VOLUME_OFFSET, volume);
    }
    pub fn load_volume(&mut self) -> u8 {
        self.read_u8(VOLUME_OFFSET)
    }
    pub fn save_input(&mut self, input: u8) {
        self.write_u8(INPUT_OFFSET, input);
    }
    pub fn load_input(&mut self) -> u8 {
        self.read_u8(INPUT_OFFSET)
    }

    fn write_u8(&mut self, offset: u32, data: u8) {
        let mut bytes = [0; ERASE_SIZE];
        bytes[0] = data;
        defmt::unwrap!(self.flash.blocking_erase(
            ADDR_OFFSET + offset,
            ADDR_OFFSET + offset + bytes.len() as u32
        ));
        defmt::unwrap!(self.flash.blocking_write(ADDR_OFFSET + offset, &bytes));
    }

    fn read_u8(&mut self, offset: u32) -> u8 {
        let mut bytes = [0; ERASE_SIZE];
        defmt::unwrap!(self.flash.blocking_read(ADDR_OFFSET + offset, &mut bytes));
        bytes[0]
    }
}
