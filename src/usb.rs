use defmt::{error, info, warn};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Sender};

use crate::Command;

use heapless::Vec;
use rsplayer_wire::{HostToFw, MAX_FRAME};

#[embassy_executor::task]
pub async fn listen_usb_commands(
    control: Sender<'static, CriticalSectionRawMutex, Command, 64>,
    mut usb_rx: embassy_usb::class::cdc_acm::Receiver<
        'static,
        embassy_rp::usb::Driver<'static, embassy_rp::peripherals::USB>,
    >,
) {
    let mut buf = [0u8; 64];
    let mut frame: Vec<u8, MAX_FRAME> = Vec::new();

    loop {
        usb_rx.wait_connection().await;
        info!("Connected");
        frame.clear();
        loop {
            match usb_rx.read_packet(&mut buf).await {
                Ok(0) => {
                    info!("Disconnected");
                    break;
                }
                Ok(n) => {
                    for &byte in &buf[..n] {
                        if byte == 0x00 {
                            // End of COBS frame: decode in place.
                            if !frame.is_empty() {
                                match postcard::from_bytes_cobs::<HostToFw>(&mut frame) {
                                    Ok(msg) => {
                                        if let Some(cmd) = host_to_fw_to_command(msg) {
                                            control.send(cmd).await;
                                        }
                                    }
                                    Err(_) => {
                                        warn!("Failed to decode HostToFw frame");
                                    }
                                }
                            }
                            frame.clear();
                        } else if frame.push(byte).is_err() {
                            // Frame longer than MAX_FRAME — resync at the next 0x00.
                            warn!("USB frame overflow, dropping");
                            frame.clear();
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

fn host_to_fw_to_command(msg: HostToFw) -> Option<Command> {
    Some(match msg {
        HostToFw::SetVolume(v) => Command::SetVolume(v),
        HostToFw::VolumeUp => Command::VolumeUp,
        HostToFw::VolumeDown => Command::VolumeDown,
        HostToFw::QueryVolume => Command::QueryCurrentVolume,
        HostToFw::PowerOn => Command::PowerOn,
        HostToFw::PowerOff => Command::PowerOff,
        HostToFw::Track { title, artist, album } => {
            Command::UpdateTrackInfo { title, artist, album }
        }
        HostToFw::Progress { current, total, percent } => {
            Command::UpdateProgress { current, total, percent }
        }
        HostToFw::Vu { left, right } => Command::UpdateVU { left, right },
        HostToFw::PlaybackMode(mode) => Command::UpdatePlaybackMode(mode),
    })
}
