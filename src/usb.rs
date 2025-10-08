use defmt::{error, info};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Sender};

use crate::Command;

use heapless::Vec;

#[embassy_executor::task]
pub async fn listen_usb_commands(
    control: Sender<'static, ThreadModeRawMutex, Command, 64>,
    mut usb_rx: embassy_usb::class::cdc_acm::Receiver<'static, embassy_rp::usb::Driver<'static, embassy_rp::peripherals::USB>>
) {
    let mut buf = [0; 64];
    let mut received_data: Vec<u8, 128> = Vec::new();

    loop {
        usb_rx.wait_connection().await;
        info!("Connected");
        loop {
            match usb_rx.read_packet(&mut buf).await {
                Ok(0) => {
                    info!("Disconnected");
                    break;
                }
                Ok(n) => {
                    for &byte in &buf[..n] {
                        if byte == b'\n' {
                            if let Ok(received_str) = core::str::from_utf8(&received_data) {
                                let received_str = received_str.trim();
                                info!("Received str: {}", received_str);
                                if received_str.starts_with("SetVol(")
                                    && received_str.ends_with(")")
                                {
                                    if let Some(vol_str) = received_str
                                        .strip_prefix("SetVol(")
                                        .and_then(|s| s.strip_suffix(")"))
                                    {
                                        if let Ok(vol) = vol_str.parse::<u8>() {
                                            control.send(Command::SetVolume(vol)).await;
                                        }
                                    }
                                } else if received_str.starts_with("QueryCurVolume") {
                                    control.send(Command::QueryCurrentVolume).await;
                                } else if received_str == "VolUp" {
                                    control.send(Command::VolumeUp).await;
                                } else if received_str == "VolDown" {
                                    control.send(Command::VolumeDown).await;
                                }
                            }
                            received_data.clear();
                        } else {
                            received_data.push(byte).ok();
                        }
                    }
                }
                Err(e) => {
                    error!("USB read error: {:?}", e);
                    break;
                }
            }
        }
    }
}
