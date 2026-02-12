use defmt::debug;
use embassy_futures::select::{select, select3, select4};
use embassy_rp::gpio::Input;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Sender};
use embassy_time::Timer;

use crate::dac::common::SampleRate;
use crate::{AmaneroPins, Command};

pub static REFRESH_SAMPLE_RATE: Signal<CriticalSectionRawMutex, ()> = Signal::new();

pub struct Amanero {
    dsd_on: Input<'static>,
    mute_en: Input<'static>,
    f0: Input<'static>,
    f1: Input<'static>,
    f2: Input<'static>,
    f3: Input<'static>,
}

impl Amanero {
    pub fn new(pins: AmaneroPins) -> Self {
        Amanero {
            dsd_on: Input::new(pins.dsd_on, embassy_rp::gpio::Pull::Down),
            mute_en: Input::new(pins.mute_en, embassy_rp::gpio::Pull::Down),
            f0: Input::new(pins.f0, embassy_rp::gpio::Pull::Down),
            f1: Input::new(pins.f1, embassy_rp::gpio::Pull::Down),
            f2: Input::new(pins.f2, embassy_rp::gpio::Pull::Down),
            f3: Input::new(pins.f3, embassy_rp::gpio::Pull::Down),
        }
    }

    pub fn read_sample_rate(&self) -> SampleRate {
        let is_dsd = self.dsd_on.is_high();
        let mute_en = self.mute_en.is_high();
        let f0 = self.f0.is_high();
        let f1 = self.f1.is_high();
        let f2 = self.f2.is_high();
        let f3 = self.f3.is_high();

        debug!(
            "amanero pins state: dsd_on: {}, f0:{}, f1:{}, f2:{}, f3:{}",
            is_dsd, f0, f1, f2, f3
        );

        match (mute_en, is_dsd, f3, f2, f1, f0) {
            // PCM
            (false, false, false, false, false, false) => SampleRate::Pcm32,
            (false, false, false, false, false, true) => SampleRate::Pcm441,
            (false, false, false, false, true, false) => SampleRate::Pcm48,
            (false, false, false, false, true, true) => SampleRate::Pcm882,
            (false, false, false, true, false, false) => SampleRate::Pcm96,
            (false, false, false, true, false, true) => SampleRate::Pcm1764,
            (false, false, false, true, true, false) => SampleRate::Pcm192,
            (false, false, false, true, true, true) => SampleRate::Pcm3528,
            (false, false, true, false, false, false) => SampleRate::Pcm384,
            (false, false, true, false, false, true) => SampleRate::Pcm7056,
            (false, false, true, false, true, false) => SampleRate::Pcm768,
            (false, false, true, false, true, true) => SampleRate::Pcm14112,
            (false, false, true, true, false, false) => SampleRate::Pcm1536,

            // DSD
            (false, true, true, false, false, true) => SampleRate::Dsd64,
            (false, true, true, false, true, false) => SampleRate::Dsd128,
            (false, true, true, false, true, true) => SampleRate::Dsd256,
            (false, true, true, true, false, false) => SampleRate::Dsd512,
            (false, true, true, true, false, true) => SampleRate::Dsd1024,

            // Defaults
            (..) => SampleRate::Unknown,
        }
    }
}
#[embassy_executor::task]
pub async fn listen_pin_changes(
    control: Sender<'static, ThreadModeRawMutex, Command, 64>,
    mut amanero: Amanero,
) {
    Timer::after_millis(500).await;

    // Initial check
    let initial_rate = amanero.read_sample_rate();
    if initial_rate != SampleRate::Unknown {
        debug!("amanero send initial rate command: {}", initial_rate);
        control.send(Command::UpdateSampleRate(initial_rate)).await;
    }

    loop {
        match select(
            REFRESH_SAMPLE_RATE.wait(),
            select3(
                amanero.dsd_on.wait_for_any_edge(),
                amanero.mute_en.wait_for_any_edge(),
                select4(
                    amanero.f0.wait_for_any_edge(),
                    amanero.f1.wait_for_any_edge(),
                    amanero.f2.wait_for_any_edge(),
                    amanero.f3.wait_for_any_edge(),
                ),
            ),
        )
        .await
        {
            _ => {
                let sample_rate = amanero.read_sample_rate();
                debug!("amanero send update rate command: {}", sample_rate);
                if sample_rate != SampleRate::Unknown {
                    control.send(Command::UpdateSampleRate(sample_rate)).await;
                }
            }
        }
    }
}
