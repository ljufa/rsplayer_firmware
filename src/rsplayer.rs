use defmt::{debug, error};
use embassy_rp::{peripherals::USB, usb::Driver};
use embassy_time::{with_timeout, Duration};
use embassy_usb::class::cdc_acm::Sender;
use rsplayer_wire::{FwPlayerCmd, FwToHost, MAX_FRAME};

pub struct RsPlayer {
    usb_sender: Sender<'static, Driver<'static, USB>>,
}

impl RsPlayer {
    pub fn new(usb_sender: Sender<'static, Driver<'static, USB>>) -> Self {
        RsPlayer { usb_sender }
    }

    pub async fn send(&mut self, msg: &FwToHost) {
        let mut buf = [0u8; MAX_FRAME];
        let frame = match postcard::to_slice_cobs(msg, &mut buf) {
            Ok(f) => f,
            Err(_) => {
                error!("postcard encode failed");
                return;
            }
        };

        if with_timeout(
            Duration::from_millis(100),
            self.usb_sender.wait_connection(),
        )
        .await
        .is_err()
        {
            debug!("USB not connected (timeout), skipping");
            return;
        }

        // Frame is at most MAX_FRAME bytes; chunk into max 64-byte USB packets.
        for chunk in frame.chunks(64) {
            match with_timeout(
                Duration::from_millis(500),
                self.usb_sender.write_packet(chunk),
            )
            .await
            {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => {
                    error!("Failed to write packet: {}", e);
                    return;
                }
                Err(_) => {
                    error!("Write packet timed out");
                    return;
                }
            }
        }
    }

    pub async fn send_player(&mut self, cmd: FwPlayerCmd) {
        self.send(&FwToHost::Player(cmd)).await;
    }

    pub async fn send_current_volume(&mut self, vol: u8) {
        self.send(&FwToHost::Volume(vol)).await;
    }

    pub async fn send_power_state(&mut self, is_on: bool) {
        self.send(&FwToHost::Power(is_on)).await;
    }
}
