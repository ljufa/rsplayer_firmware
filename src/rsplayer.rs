use core::fmt::Write;
use defmt::{debug, error, info};
use embassy_rp::{peripherals::USB, usb::Driver};
use embassy_usb::class::cdc_acm::Sender;

use crate::fmtbuf::FmtBuf;

pub struct RsPlayer {
    usb_sender: Sender<'static, Driver<'static, USB>>,
}
impl RsPlayer {
    pub fn new(usb_sender: Sender<'static, Driver<'static, USB>>) -> Self {
        RsPlayer { usb_sender }
    }
    pub async fn send_command(&mut self, cmd: &str) {
        if cmd.len() > 16 {
            error!("Command too long");
            return;
        }
        let buff = &mut FmtBuf::new();
        _ = writeln!(buff, "{}", cmd);
        debug!("Sending command: {}", buff.as_str());
        self.usb_sender.wait_connection().await;
        match self.usb_sender.write_packet(buff.as_str().as_bytes()).await {
            Ok(_) => debug!("Write packet success"),
            Err(e) => error!("Failed to write packet: {}", e),
        }
    }

    pub async fn send_current_volume(&mut self, vol: u8) {
        let buff = &mut FmtBuf::new();
        _ = write!(buff, "CurVolume={}", vol);
        info!("Sending command: {}", buff.as_str());
        self.send_command(buff.as_str()).await;
    }
}
