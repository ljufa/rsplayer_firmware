use core::fmt::Write;
use defmt::{debug, error, info};
use embassy_rp::{peripherals::USB, usb::Driver};
use embassy_usb::class::cdc_acm::Sender;
use embassy_time::{with_timeout, Duration};

use crate::fmtbuf::FmtBuf;

pub struct RsPlayer {
    usb_sender: Sender<'static, Driver<'static, USB>>,
}
impl RsPlayer {
    pub fn new(usb_sender: Sender<'static, Driver<'static, USB>>) -> Self {
        RsPlayer { usb_sender }
    }
    pub async fn send_command(&mut self, cmd: &str) {
        if cmd.len() > 64 {
            error!("Command [{}] too long", cmd);
            return;
        }
        let buff = &mut FmtBuf::new();
        _ = writeln!(buff, "{}", cmd);
        debug!("Sending command: {}", buff.as_str());
        
        if with_timeout(Duration::from_millis(100), self.usb_sender.wait_connection()).await.is_err() {
            debug!("USB not connected (timeout), skipping command sending");
            return;
        }

        match with_timeout(Duration::from_millis(500), self.usb_sender.write_packet(buff.as_str().as_bytes())).await {
            Ok(Ok(_)) => debug!("Write packet success"),
            Ok(Err(e)) => error!("Failed to write packet: {}", e),
            Err(_) => error!("Write packet timed out"),
        }
    }

    pub async fn send_current_volume(&mut self, vol: u8) {
        let buff = &mut FmtBuf::new();
        _ = write!(buff, "CurVolume={}", vol);
        info!("Sending command: {}", buff.as_str());
        self.send_command(buff.as_str()).await;
    }
}
