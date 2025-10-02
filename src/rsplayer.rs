use defmt::{debug, error};
use embassy_rp::{
    peripherals::UART0,
    uart::{Async, UartTx},
};

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
}
