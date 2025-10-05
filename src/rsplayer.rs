use core::fmt::Write;
use defmt::{debug, error, info};
use embassy_rp::{
    peripherals::UART0,
    uart::{Async, UartTx},
};

use crate::fmtbuf::FmtBuf;

pub struct RsPlayer {
    uart: UartTx<'static, UART0, Async>,
}
impl RsPlayer {
    pub fn new(uart: UartTx<'static, UART0, Async>) -> Self {
        RsPlayer { uart }
    }
    pub fn send_command(&mut self, cmd: &str) {
        if cmd.len() > 16 {
            error!("Command too long");
            return;
        }
        let mut cmd_bytes = [b' '; 16]; // Fill with spaces by default

        if cmd.len() <= 16 {
            cmd_bytes[..cmd.len()].copy_from_slice(cmd.as_bytes());
        } else {
            cmd_bytes.copy_from_slice(&cmd.as_bytes()[..16]); // Truncate if too long
        }
        debug!("Sending command: {:?}", cmd_bytes);
        self.uart.blocking_write(&cmd_bytes).unwrap();
    }

    pub fn send_current_volume(&mut self, vol: u8) {
        let buff = &mut FmtBuf::new();
        _ = write!(buff, "CurVolume={}", vol);
        info!("Sending command: {}", buff.as_str());
        self.send_command(buff.as_str());
    }
    
}
