use defmt::{error, info};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Sender};

use crate::Command;
use crate::PlaybackMode;

use heapless::{String, Vec};

#[embassy_executor::task]
pub async fn listen_usb_commands(
    control: Sender<'static, ThreadModeRawMutex, Command, 64>,
    mut usb_rx: embassy_usb::class::cdc_acm::Receiver<'static, embassy_rp::usb::Driver<'static, embassy_rp::peripherals::USB>>
) {
    let mut buf = [0; 64];
    let mut received_data: Vec<u8, 512> = Vec::new();

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
                                } else if received_str.starts_with("SetVU(") && received_str.ends_with(")") {
                                    if let Some(content) = received_str.strip_prefix("SetVU(").and_then(|s| s.strip_suffix(")")) {
                                        let mut parts = content.split('|');
                                        if let (Some(l_str), Some(r_str)) = (parts.next(), parts.next()) {
                                            if let (Ok(left), Ok(right)) = (l_str.parse::<u8>(), r_str.parse::<u8>()) {
                                                control.send(Command::UpdateVU { left, right }).await;
                                            }
                                        }
                                    }
                                } else if received_str.starts_with("SetTrack(") && received_str.ends_with(")") {
                                    if let Some(content) = received_str.strip_prefix("SetTrack(").and_then(|s| s.strip_suffix(")")) {
                                        let mut parts = content.split('|');
                                        if let (Some(title), Some(artist), Some(album)) = (parts.next(), parts.next(), parts.next()) {
                                            let mut t: String<64> = String::new();
                                            t.push_str(title).ok();
                                            let mut a: String<64> = String::new();
                                            a.push_str(artist).ok();
                                            let mut al: String<64> = String::new();
                                            al.push_str(album).ok();
                                            
                                            control.send(Command::UpdateTrackInfo {
                                                title: t,
                                                artist: a,
                                                album: al,
                                            }).await;
                                        }
                                    }
                                } else if received_str.starts_with("SetProgress(") && received_str.ends_with(")") {
                                    if let Some(content) = received_str.strip_prefix("SetProgress(").and_then(|s| s.strip_suffix(")")) {
                                        let mut parts = content.split('|');
                                        if let (Some(curr), Some(tot), Some(pct_str)) = (parts.next(), parts.next(), parts.next()) {
                                            if let Ok(pct) = pct_str.parse::<u8>() {
                                                let mut c: String<16> = String::new();
                                                c.push_str(curr).ok();
                                                let mut t: String<16> = String::new();
                                                t.push_str(tot).ok();

                                                control.send(Command::UpdateProgress {
                                                    current: c,
                                                    total: t,
                                                    percent: pct,
                                                }).await;
                                            }
                                        }
                                    }
                                } else if received_str.starts_with("SetPlaybackMode(") && received_str.ends_with(")") {
                                    if let Some(mode_str) = received_str.strip_prefix("SetPlaybackMode(").and_then(|s| s.strip_suffix(")")) {
                                        let mode = match mode_str {
                                            "Random" => Some(PlaybackMode::Random),
                                            "Sequential" => Some(PlaybackMode::Sequential),
                                            "LoopSingle" => Some(PlaybackMode::LoopSingle),
                                            "LoopQueue" => Some(PlaybackMode::LoopQueue),
                                            _ => None,
                                        };
                                        if let Some(m) = mode {
                                            control.send(Command::UpdatePlaybackMode(m)).await;
                                        }
                                    }
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
