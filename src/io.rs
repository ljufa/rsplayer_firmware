use defmt::{error, info};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Sender};
use embassy_time::Instant;

use infrared::{protocol::Nec, Receiver};

use crate::Command;
use embassy_rp::{
    gpio::{Input, Pull},
    peripherals::{PIN_10, PIN_3, UART0},
    uart::{Async, UartRx},
};

pub mod gpio {
    use super::*;
    #[embassy_executor::task]
    pub async fn listen_mute_pin(
        control: Sender<'static, ThreadModeRawMutex, Command, 64>,
        pin10: PIN_10,
    ) {
        let mut mute_input = Input::new(pin10, Pull::None);
        loop {
            mute_input.wait_for_any_edge().await;
            if mute_input.is_high() {
                info!("Mute pin high");
                control.send(Command::Mute).await;
            }
            if mute_input.is_low() {
                info!("Mute pin low");
                control.send(Command::Unmute).await;
            }
        }
    }
}
pub mod uart {
    use super::*;
    #[embassy_executor::task]
    pub async fn listen_uart_commands(
        control: Sender<'static, ThreadModeRawMutex, Command, 64>,
        mut uart: UartRx<'static, UART0, Async>,
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
                    }
                }
                Err(_) => {
                    error!("UART read error");
                }
            }
        }
    }
}

pub mod ir {
    use super::*;
    #[embassy_executor::task]
    pub async fn listen_ir_receiver(
        control: Sender<'static, ThreadModeRawMutex, Command, 64>,
        pin3: PIN_3,
    ) {
        let mut ir_pin = Input::new(pin3, Pull::Down);
        let mut ir_recv: Receiver<Nec> = Receiver::new(1_000_000);
        let mut lastedge = Instant::now();
        let mut repeat_cnt = 0;
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
                    //home button
                    83 => {
                        if !cmd.repeat {
                            control.send(Command::ToggleDacDsdPcmMode).await
                        }
                    }
                    // power button
                    81 => {
                        if cmd.repeat {
                            repeat_cnt += 1;
                            if repeat_cnt > 10 {
                                control.send(Command::TogglePower).await;
                                repeat_cnt = 0;
                            }
                        } else {
                            repeat_cnt = 0;
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

                    _ => {}
                }
            }
            lastedge = now;
        }
    }
}

pub mod rotary {
    use super::*;
    use embassy_rp::{
        gpio::Level,
        peripherals::{PIN_21, PIO0},
        pio_programs::rotary_encoder::{Direction, PioEncoder},
    };
    use embassy_time::{with_deadline, Duration, Timer};

    #[embassy_executor::task]
    pub async fn listen_rotary_encoder(
        control: Sender<'static, ThreadModeRawMutex, Command, 64>,
        mut encoder: PioEncoder<'static, PIO0, 0>,
    ) {
        loop {
            match encoder.read().await {
                Direction::Clockwise => control.send(Command::VolumeUp).await,
                Direction::CounterClockwise => control.send(Command::VolumeDown).await,
            };
        }
    }

    #[embassy_executor::task]
    pub async fn listen_rotary_encoder_button(
        control: Sender<'static, ThreadModeRawMutex, Command, 64>,
        pin: PIN_21,
    ) {
        let btn_pin = Input::new(pin, Pull::Up);
        let mut btn = Debouncer::new(btn_pin, Duration::from_millis(20));
        loop {
            btn.debounce().await;
            let start = Instant::now();

            match with_deadline(start + Duration::from_secs(1), btn.debounce()).await {
                // Button Released < 1s
                Ok(_) => {
                    continue;
                }
                // button held for > 1s
                Err(_) => {
                    control.send(Command::TogglePlay).await;
                    info!("Button Held");
                }
            }

            match with_deadline(start + Duration::from_secs(5), btn.debounce()).await {
                Ok(_) => {
                    continue;
                }
                // button held for > >5s
                Err(_) => {
                    info!("Button Long Held");
                    control.send(Command::TogglePower).await;
                }
            }

            btn.debounce().await;
        }
    }

    pub struct Debouncer<'a> {
        input: Input<'a>,
        debounce: Duration,
    }

    impl<'a> Debouncer<'a> {
        pub fn new(input: Input<'a>, debounce: Duration) -> Self {
            Self { input, debounce }
        }

        pub async fn debounce(&mut self) -> Level {
            loop {
                let l1 = self.input.get_level();

                self.input.wait_for_any_edge().await;

                Timer::after(self.debounce).await;

                let l2 = self.input.get_level();
                if l1 != l2 {
                    break l2;
                }
            }
        }
    }
}
