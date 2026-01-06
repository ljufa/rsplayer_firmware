use crate::Command;
use defmt::debug;
use embassy_rp::gpio::{Input, Pull};
use embassy_rp::peripherals::PIN_11;
use embassy_rp::Peri;
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::Sender;
#[embassy_executor::task]
    pub async fn listen_mute_pin(
        control: Sender<'static, ThreadModeRawMutex, Command, 64>,
        pin10: Peri<'static,PIN_11>,
    ) {
        let mut mute_input = Input::new(pin10, Pull::None);
        loop {
            mute_input.wait_for_any_edge().await;
            if mute_input.is_high() {
                debug!("Mute pin high");
                control.send(Command::MutePluse(300)).await;
            }
        }
    }
