use defmt::info;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Sender};
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
    control: Sender<'static, CriticalSectionRawMutex, Command, 64>,
    pin3: Peri<'static, PIN_3>,
) {
    let mut ir_pin = Input::new(pin3, Pull::Down);
    let mut ir_recv: Receiver<Nec> = Receiver::new(1_000_000);
    let mut lastedge = Instant::now();
    // Key-repeat state for the volume buttons: a click steps once; NEC
    // repeat frames (~every 108ms while held) only ramp the volume after
    // the button has been held for a while, and throttled — otherwise a
    // normal click lands 1-2 repeats and jumps several steps.
    let mut vol_press_start = Instant::now();
    let mut last_vol_step = Instant::now();
    loop {
        ir_pin.wait_for_any_edge().await;
        let rising = ir_pin.is_high();
        let now = Instant::now();
        let dur = now.checked_duration_since(lastedge).unwrap();
        // Saturate instead of unwrap: after >71 min without IR activity the
        // gap overflows u32 µs and the old `.unwrap()` paniced on the first
        // press. A huge dt just resets the decoder state machine.
        let dt: u32 = dur.as_micros().try_into().unwrap_or(u32::MAX);
        if let Ok(Some(cmd)) = ir_recv.event(dt, !rising) {
            info!("cmd: {}, addr: {}, rep: {}", cmd.cmd, cmd.addr, cmd.repeat);
            if cmd.addr != 128 {
                continue;
            }
            match cmd.cmd {
                38 | 40 => {
                    let fire = if cmd.repeat {
                        now.duration_since(vol_press_start).as_millis() >= 400
                            && now.duration_since(last_vol_step).as_millis() >= 120
                    } else {
                        vol_press_start = now;
                        true
                    };
                    if fire {
                        last_vol_step = now;
                        control
                            .send(if cmd.cmd == 38 { Command::VolumeUp } else { Command::VolumeDown })
                            .await;
                    }
                }
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
                        control.send(Command::ToggleDisplayMode).await
                    }
                }
                // VOL+ button
                78 => {
                    if !cmd.repeat {
                        control.send(Command::ToggleRandomPlay).await
                    }
                }
                187 => {
                    if !cmd.repeat {
                        control.send(Command::SeekForward).await
                    }
                }
                189 => {
                    if !cmd.repeat {
                        control.send(Command::SeekBackward).await
                    }
                }

                _ => {}
            }
        }
        lastedge = now;
    }
}
