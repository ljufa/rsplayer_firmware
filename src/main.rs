#![no_std]
#![no_main]
#![allow(async_fn_in_trait)]
use core::fmt::Write;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering::SeqCst;

use assign_resources::assign_resources;
use dac::Dac;
use defmt::unwrap;
use defmt::{debug, info};
use display::OledDisplay;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{self, I2C1, UART0};
use embassy_rp::pio::Pio;
use embassy_rp::pio_programs::rotary_encoder::{PioEncoder, PioEncoderProgram};
use embassy_rp::uart::Uart;
use embassy_rp::{bind_interrupts, uart};
use embassy_sync::mutex::Mutex;
use embassy_time::{block_for, Duration, Timer};

use embassy_executor::Executor;

use crate::fmtbuf::FmtBuf;
use embassy_rp::multicore::{spawn_core1, Stack};
use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, ThreadModeRawMutex};
use embassy_sync::channel::Channel;
use heapless::String;
use static_cell::StaticCell;

use embassy_rp::peripherals::PIO0;
use {defmt_rtt as _, panic_probe as _};
mod dac;
mod display;

mod flash;
mod fmtbuf;
mod i2c_helper;
mod io;
mod rsplayer;
bind_interrupts!(struct IrqsI2c {
    I2C1_IRQ => embassy_rp::i2c::InterruptHandler<I2C1>;
});

bind_interrupts!(struct IrqsUart {
    UART0_IRQ => embassy_rp::uart::InterruptHandler<UART0>;
});
bind_interrupts!(struct IrqsPio {
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<PIO0>;
});

assign_resources! {
    out: OutputPins {
        pin0: PIN_0,
        pin1: PIN_1,
        pin6: PIN_6,
        pin7: PIN_7,
    }
    dac: DacResources{
        i2c: I2C1,
        pin15_i2c_scl: PIN_15,
        pin14_i2c_sda: PIN_14,
        pin2_dac_pdc: PIN_2,
    }
    display: DisplayResources {
        spi0: SPI0,
        dmach3: DMA_CH3,
        pin18_spi_sck: PIN_18,
        pin19_spi_tx: PIN_19,
        pin20_spi_rst: PIN_20,
        pin22_blk_gnd: PIN_22,
        pin5_dummy_cs: PIN_5,
    }
    flash: FlashResources {
        flash: FLASH,
        dma_ch4: DMA_CH4,
    }
    rotary: RotaryResources {
        pin16_a: PIN_16,
        pin17_b: PIN_17,
        pin21_sw: PIN_21,
    }
}

static DISPLAY: Mutex<CriticalSectionRawMutex, Option<OledDisplay>> = Mutex::new(None);

#[derive(Eq, PartialEq, PartialOrd)]
enum Command {
    Mute,
    Unmute,
    TogglePower,
    VolumeUp,
    VolumeDown,
    SetVolume(u8),
    ToggleInput,

    Next,
    Prev,
    TogglePlay,
    NextDacSoundSetting,
    NextDacFilterType,
    ToggleDacDsdPcmMode,
    ToggleDacDsdDclkPolarity,
    ToggleDacDsdCutoffFreqFilter,
    ToggleDacDsdDclksClock,
    QueryCurrentVolume,
    ToggleRandomPlay
}

static CMD_CHANNEL: Channel<ThreadModeRawMutex, Command, 64> = Channel::new();
static mut CORE1_STACK: Stack<8096> = Stack::new();
static EXECUTOR0: StaticCell<Executor> = StaticCell::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();
static POWER_ON: AtomicBool = AtomicBool::new(false);

#[cortex_m_rt::entry]
fn main() -> ! {
    let php = embassy_rp::init(Default::default());

    // add some delay to give an attached debug probe time to parse the
    // defmt RTT header. Reading that header might touch flash memory, which
    // interferes with flash write operations.
    // https://github.com/knurling-rs/defmt/pull/683
    block_for(Duration::from_millis(50));

    let mut config = uart::Config::default();
    config.baudrate = 115200;

    let uart = Uart::new(
        php.UART0,
        php.PIN_12,
        php.PIN_13,
        IrqsUart,
        php.DMA_CH0,
        php.DMA_CH1,
        config,
    );
    let (uart_tx, uart_rx) = uart.split();

    let res = split_resources!(php);
    let dac = Dac::new(res.dac);

    let rsplayer = rsplayer::RsPlayer::new(uart_tx);

    let flash = flash::Storage::new(res.flash);

    let Pio {
        mut common, sm0, ..
    } = Pio::new(php.PIO0, IrqsPio);

    let prg = PioEncoderProgram::new(&mut common);
    let encoder: PioEncoder<'_, PIO0, 0> = PioEncoder::new(
        &mut common,
        sm0,
        res.rotary.pin16_a,
        res.rotary.pin17_b,
        &prg,
    );

    // start command processing on core1
    spawn_core1(
        php.CORE1,
        unsafe { &mut *core::ptr::addr_of_mut!(CORE1_STACK) },
        move || {
            let executor1 = EXECUTOR1.init(Executor::new());
            executor1.run(|spawner| {
                unwrap!(spawner.spawn(io::uart::listen_uart_commands(
                    CMD_CHANNEL.sender(),
                    uart_rx
                )));
                unwrap!(spawner.spawn(io::rotary::listen_rotary_encoder(
                    CMD_CHANNEL.sender(),
                    encoder
                )));
                unwrap!(spawner.spawn(io::rotary::listen_rotary_encoder_button(
                    CMD_CHANNEL.sender(),
                    res.rotary.pin21_sw
                )));
                unwrap!(spawner.spawn(io::ir::listen_ir_receiver(CMD_CHANNEL.sender(), php.PIN_3)));
            });
        },
    );

    // receive commands on core0
    let executor0 = EXECUTOR0.init(Executor::new());
    executor0.run(|spawner| {
        unwrap!(spawner.spawn(dim_display()));
        unwrap!(spawner.spawn(process_commands(dac, rsplayer, res.out, res.display, flash)))
    });
}

#[embassy_executor::task]
pub async fn dim_display() {
    loop {
        if let Some(disp) = DISPLAY.lock().await.as_mut() {
            let last_updated = disp.last_update.elapsed().as_secs();
            if last_updated > 20 && last_updated < 55 {
                disp.turn_off_backlight();
            }
        }
        Timer::after_millis(2000).await;
    }
}

#[embassy_executor::task]
pub async fn process_commands(
    mut dac: Dac,
    mut rsplayer: rsplayer::RsPlayer,
    out: OutputPins,
    display_resources: DisplayResources,
    mut flash: flash::Storage,
) {
    // set power pins to low
    let mut pwr_pi_relay = Output::new(out.pin7, Level::Low);
    let mut pwr_psu_relay = Output::new(out.pin1, Level::Low);
    let mut mute_out_relay = Output::new(out.pin0, Level::Low);
    let mut input_signal_relay = Output::new(out.pin6, Level::Low);
    {
        DISPLAY
            .lock()
            .await
            .replace(OledDisplay::new(display_resources));
    }
    DISPLAY.lock().await.as_mut().unwrap().draw_powered_off();
    let mut buff = String::<32>::new();

    loop {
        let cmd = CMD_CHANNEL.receive().await;
        let is_power_on = POWER_ON.load(SeqCst);
        if cmd != Command::TogglePower && !is_power_on {
            info!("Power is off, ignoring command");
            continue;
        }
        let mut disp = DISPLAY.lock().await;
        let disp = disp.as_mut().unwrap();
        match cmd {
            Command::TogglePower => {
                info!("got TogglePower");
                if !is_power_on {
                    debug!("Powering on");
                    pwr_psu_relay.set_high();
                    POWER_ON.store(true, SeqCst);
                    Timer::after_millis(500).await;
                    dac.initialize().await;
                    pwr_pi_relay.set_high();
                    disp.clear();
                    disp.draw_powering_on();
                    // wait for RPI to boot
                    Timer::after_millis(20000).await;
                    disp.clear();
                    let vol = flash.load_volume();
                    debug!("Stored volume: {}", vol);
                    dac.set_volume(vol).await;
                    disp.draw_volume(vol, &mut buff);
                    let input = flash.load_input();
                    debug!("Stored input: {}", input);
                    if input == 0 {
                        input_signal_relay.set_low();
                        disp.draw_input_signal("I2S", &mut buff);
                    } else {
                        input_signal_relay.set_high();
                        disp.draw_input_signal("Optical", &mut buff);
                    }
                    mute_out_relay.set_high();
                    debug!("Powered on");
                } else {
                    debug!("Powering off");
                    mute_out_relay.set_low();
                    rsplayer.send_command("PowerOff");
                    Timer::after_millis(20000).await;
                    pwr_psu_relay.set_low();
                    pwr_pi_relay.set_low();
                    POWER_ON.store(false, SeqCst);
                    disp.draw_powered_off();
                    debug!("Powered off");
                }
                Timer::after_millis(2000).await;
            }
            Command::Mute => {
                info!("got Mute");
                mute_out_relay.set_low();
            }
            Command::Unmute => {
                info!("got Unmute");
                mute_out_relay.set_high();
            }
            Command::VolumeUp => {
                info!("got VolumeUp");
                let new_val = dac.volume_up().await;
                flash.save_volume(new_val);
                disp.draw_volume(new_val, &mut buff);
                rsplayer.send_current_volume(new_val);
            }
            Command::VolumeDown => {
                info!("got VolumeDown");
                let new_val = dac.volume_down().await;
                flash.save_volume(new_val);
                disp.draw_volume(new_val, &mut buff);
                rsplayer.send_current_volume(new_val);
            }
            Command::SetVolume(vol) => {
                info!("Received SetVolume({})", vol);
                dac.set_volume(vol).await;
                flash.save_volume(vol);
                disp.draw_volume(vol, &mut buff);
                rsplayer.send_current_volume(vol);
            }
            Command::ToggleRandomPlay => {
                info!("got ToggleRandomPlay");
                rsplayer.send_command("RandomToggle");
            }
            Command::ToggleInput => {
                // select optical or coaxial input
                if input_signal_relay.is_set_low() {
                    info!("Input signal relay set high");
                    rsplayer.send_command("Stop");
                    input_signal_relay.set_high();
                    flash.save_input(1);
                    disp.draw_input_signal("Optical", &mut buff);
                }
                // select i2s input
                else {
                    info!("Input signal relay set low");
                    rsplayer.send_command("Play");
                    input_signal_relay.set_low();
                    flash.save_input(0);
                    disp.draw_input_signal("I2S", &mut buff);
                }
            }
            Command::Next => {
                rsplayer.send_command("Next");
            }
            Command::Prev => {
                rsplayer.send_command("Prev");
            }
            Command::TogglePlay => {
                rsplayer.send_command("TogglePlay");
            }
            Command::NextDacSoundSetting => {
                info!("got NextDacSoundSetting");
                dac.next_sound_setting().await;
            }
            Command::NextDacFilterType => {
                info!("got NextDacFilterType");
                dac.next_filter().await;
            }
            Command::ToggleDacDsdPcmMode => {
                info!("got ToggleDacDsdPcmMode");
                dac.toggle_dsd_pcm().await;
            }
            Command::ToggleDacDsdDclkPolarity => {
                info!("got ToggleDacDsdDclkPolarity");
                dac.toggle_dsd_dclk_polarity().await;
            }
            Command::ToggleDacDsdCutoffFreqFilter => {
                info!("got ToggleDacDsdCutoffFreqFilter");
                dac.toggle_dsd_cutoff_freq_filter().await;
            }
            Command::ToggleDacDsdDclksClock => {
                info!("got ToggleDacDsdDclksClock");
                dac.toggle_dsd_dcks_clock().await;
            }
            Command::QueryCurrentVolume => {
                let vol = flash.load_volume();
                rsplayer.send_current_volume(vol);
            }
        }
    }
}
