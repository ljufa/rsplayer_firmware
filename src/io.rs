use defmt::info;
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Sender};
use embassy_time::Instant;


use crate::Command;
use embassy_rp::{
    gpio::{Input, Pull},
    peripherals::PIN_10,
};

pub mod gpio {
    use embassy_rp::Peri;

    use super::*;
    #[embassy_executor::task]
    pub async fn listen_mute_pin(
        control: Sender<'static, ThreadModeRawMutex, Command, 64>,
        pin10: Peri<'static,PIN_10>,
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


pub mod rotary {
    use super::*;
    use embassy_rp::{
        gpio::Level,
        peripherals::{PIN_21, PIO0},
        pio_programs::rotary_encoder::{Direction, PioEncoder}, Peri,
    };
    use embassy_time::{with_deadline, with_timeout, Duration, Timer};

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
        pin: Peri<'static, PIN_21>,
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

                if with_timeout(Duration::from_millis(100), self.input.wait_for_any_edge())
                    .await
                    .is_err()
                {
                    continue;
                };

                Timer::after(self.debounce).await;

                let l2 = self.input.get_level();
                if l1 != l2 {
                    break l2;
                }
            }
        }
    }
}
