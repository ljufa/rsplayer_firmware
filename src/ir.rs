use defmt::info;
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Sender};
use embassy_time::Instant;

use infrared::{protocol::Nec, Receiver};

use crate::Command;
use embassy_rp::{
    gpio::{Input, Pull},
    peripherals::PIN_3,
    Peri,
};
#[embassy_executor::task]
pub async fn listen_ir_receiver(
    control: Sender<'static, ThreadModeRawMutex, Command, 64>,
    pin3: Peri<'static, PIN_3>,
) {
    let mut ir_pin = Input::new(pin3, Pull::Down);
    let mut ir_recv: Receiver<Nec> = Receiver::new(1_000_000);
    let mut lastedge = Instant::now();
    loop {
        ir_pin.wait_for_any_edge().await;
        let rising = ir_pin.is_high();
        let now = Instant::now();
        let dur = now.checked_duration_since(lastedge).unwrap();
        if let Ok(Some(cmd)) = ir_recv.event(dur.as_micros().try_into().unwrap(), !rising) {
            info!("cmd: {}, addr: {}, rep: {}", cmd.cmd, cmd.addr, cmd.repeat);
            if cmd.addr != 128 {
                continue;
            }
            match cmd.cmd {
                38 => control.send(Command::VolumeUp).await,
                40 => control.send(Command::VolumeDown).await,
                39 => {
                    if !cmd.repeat {
                        control.send(Command::Next).await
                    }
                }
                37 => {
                    if !cmd.repeat {
                        control.send(Command::Prev).await
                    }
                }
                13 => {
                    if !cmd.repeat {
                        control.send(Command::TogglePlay).await
                    }
                }
                // menu button
                73 => {
                    if !cmd.repeat {
                        control.send(Command::ToggleInput).await
                    }
                }
                // mouse button
                82 => {
                    if !cmd.repeat {
                        control.send(Command::NextDacFilterType).await
                    }
                }
                // return button
                27 => {
                    if !cmd.repeat {
                        control.send(Command::NextDacSoundSetting).await
                    }
                }
                // power button
                9 => {
                    if !cmd.repeat {
                        control.send(Command::TogglePower).await;
                    }
                }
                // 1 button
                49 => {
                    if !cmd.repeat {
                        control.send(Command::ToggleDacDsdDclkPolarity).await
                    }
                }
                // 2 button
                50 => {
                    if !cmd.repeat {
                        control.send(Command::ToggleDacDsdCutoffFreqFilter).await
                    }
                }
                // 3 button
                51 => {
                    if !cmd.repeat {
                        control.send(Command::ToggleDacDsdDclksClock).await
                    }
                }
                // 4 button
                52 => {
                    if !cmd.repeat {
                        control.send(Command::ToggleVUMode).await
                    }
                }
                // VOL+ button
                78 => {
                    if !cmd.repeat {
                        control.send(Command::ToggleRandomPlay).await
                    }
                }

                _ => {}
            }
        }
        lastedge = now;
    }
}
