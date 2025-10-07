    use defmt::{error, info};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Sender};


use crate::Command;
use embassy_rp::uart::{Async, UartRx};

    #[embassy_executor::task]
    pub async fn listen_uart_commands(
        control: Sender<'static, ThreadModeRawMutex, Command, 64>,
        mut uart: UartRx<'static,  Async>,
    ) {
        let mut buffer = [0u8; 16];
        loop {
            match uart.read(&mut buffer).await {
                Ok(_) => {
                    if let Ok(received_str) = core::str::from_utf8(&buffer) {
                        let received_str = received_str.trim(); // Remove trailing spaces
                        info!("Received str: {}", received_str);
                        if received_str.starts_with("SetVol(") && received_str.ends_with(")") {
                            if let Some(vol_str) = received_str
                                .strip_prefix("SetVol(")
                                .and_then(|s| s.strip_suffix(")"))
                            {
                                if let Ok(vol) = vol_str.parse::<u8>() {
                                    control.send(Command::SetVolume(vol)).await;
                                }
                            }
                        }
                        else if received_str.starts_with("QueryCurVolume") {
                            control.send(Command::QueryCurrentVolume).await;
                        }
                        else if received_str == "VolUp" {
                            control.send(Command::VolumeUp).await;
                        } else if received_str == "VolDown" {
                            control.send(Command::VolumeDown).await;
                        }
                    }
                }
                Err(_) => {
                    error!("UART read error");
                }
            }
        }
    }
  
    

