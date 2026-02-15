#![no_std]
#![no_main]
#![allow(async_fn_in_trait)]
use core::sync::atomic::AtomicBool;

use assign_resources::assign_resources;

use defmt::unwrap;
use defmt::{debug, info};
use display::{DisplayMode, OledDisplay};
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{self, I2C1, USB};
use embassy_rp::pio::Pio;
use embassy_rp::pio_programs::rotary_encoder::{PioEncoder, PioEncoderProgram};

use embassy_rp::bind_interrupts;
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_sync::mutex::Mutex;
use embassy_time::Timer;

use embassy_executor::Executor;

use embassy_rp::multicore::{spawn_core1, Stack};
use embassy_rp::Peri;
use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, ThreadModeRawMutex};
use embassy_sync::channel::Channel;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::UsbDevice;
use heapless::String;
use static_cell::StaticCell;

use crate::amanero::Amanero;
use crate::rsplayer::RsPlayer;
use embassy_rp::peripherals::PIO0;

use crate::dac::common::{Akm44xxDac, FilterType};
use dac::common::SampleRate;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, defmt::Format)]
pub enum PlaybackMode {
    Sequential,
    Random,
    LoopSingle,
    LoopQueue,
}
// Only one of these will be active based on the feature flag
#[cfg(feature = "debug")]
use panic_probe as _;
#[cfg(feature = "debug")]
use {defmt_rtt as _};

#[cfg(feature = "release")]
use panic_reset as _;

mod dac;
mod display;

mod amanero;
mod flash;
mod fmtbuf;
// mod gpio;
mod i2c_helper;
mod ir;
mod rotary;
mod rsplayer;
mod usb;

bind_interrupts!(struct IrqsI2c {
    I2C1_IRQ => embassy_rp::i2c::InterruptHandler<I2C1>;
});
bind_interrupts!(struct IrqsPio {
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<PIO0>;
});
bind_interrupts!(struct IrqsUsb {
    USBCTRL_IRQ => InterruptHandler<USB>;
});

assign_resources! {
    amanero: AmaneroPins{
        dsd_on: PIN_10,
        mute_en: PIN_11,
        f0: PIN_4,
        f1: PIN_5,
        f2: PIN_8,
        f3: PIN_9,
    }
    out: OutputPins {
        pin0: PIN_0,
        pin1: PIN_1,
        pin6: PIN_6,
        // pin7: PIN_7,
    }
    dac: DacResources{
        i2c: I2C1,
        pin15_i2c_scl: PIN_15,
        pin14_i2c_sda: PIN_14,
        pin2_dac_pdn: PIN_2,
    }
    display: DisplayResources {
        spi0: SPI0,
        dmach3: DMA_CH3,
        pin7_spi_dc: PIN_7,
        pin18_spi_sck: PIN_18,
        pin19_spi_tx: PIN_19,
        pin20_spi_rst: PIN_20,
        pin22_blk_gnd: PIN_22,
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

#[derive(Eq, PartialEq, PartialOrd, Debug)]
enum Command {
    UpdateSampleRate(SampleRate),
    UpdateTrackInfo {
        title: String<64>,
        artist: String<64>,
        album: String<64>,
    },
    UpdateVU {
        left: u8,
        right: u8,
    },
    UpdateProgress {
        current: String<16>,
        total: String<16>,
        percent: u8,
    },
    UpdatePlaybackMode(PlaybackMode),
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
    ToggleDacDsdDclkPolarity,
    ToggleDacDsdCutoffFreqFilter,
    ToggleDacDsdDclksClock,
    QueryCurrentVolume,
    ToggleRandomPlay,
    ToggleDisplayMode,
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
    // block_for(Duration::from_millis(150));

    let res = split_resources!(php);
    let dac = Akm44xxDac::new(res.dac);
    let amanero = Amanero::new(res.amanero);
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

    // USB
    // Create the driver, from the HAL.
    let usb_driver = Driver::new(php.USB, IrqsUsb);

    // Create embassy-usb Config
    let usb_config = {
        let mut config = embassy_usb::Config::new(0xc0fe, 0xb0b1);
        config.manufacturer = Some("RSPlayer");
        config.product = Some("rsplayer-firmware-v1.0");
        config.serial_number = Some("000001");
        config.self_powered = true;
        config.max_power = 0;
        config.max_packet_size_0 = 64;
        config
    };

    // Create embassy-usb DeviceBuilder using the driver and config.
    // It needs some buffers for building the descriptors.
    let mut usb_builder = {
        static CONFIG_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
        static BOS_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
        static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();

        let builder = embassy_usb::Builder::new(
            usb_driver,
            usb_config,
            CONFIG_DESCRIPTOR.init([0; 256]),
            BOS_DESCRIPTOR.init([0; 256]),
            &mut [], // no msos descriptors
            CONTROL_BUF.init([0; 64]),
        );
        builder
    };

    // Create classes on the builder.
    let usb_class: CdcAcmClass<'_, Driver<'_, USB>> = {
        static STATE: StaticCell<State> = StaticCell::new();
        let state = STATE.init(State::new());
        CdcAcmClass::new(&mut usb_builder, state, 64)
    };
    let (usb_tx, usb_rx) = usb_class.split();
    let rsplayer = RsPlayer::new(usb_tx);
    // Build the builder.
    let usb_device = usb_builder.build();

    // start command processing on core1
    spawn_core1(
        php.CORE1,
        unsafe { &mut *core::ptr::addr_of_mut!(CORE1_STACK) },
        move || {
            let executor1 = EXECUTOR1.init(Executor::new());
            executor1.run(|spawner| {
                unwrap!(spawner.spawn(rotary::listen_rotary_encoder(
                    CMD_CHANNEL.sender(),
                    encoder
                )));
                unwrap!(spawner.spawn(rotary::listen_rotary_encoder_button(
                    CMD_CHANNEL.sender(),
                    res.rotary.pin21_sw
                )));
                unwrap!(spawner.spawn(ir::listen_ir_receiver(CMD_CHANNEL.sender(), php.PIN_3)));
                unwrap!(spawner.spawn(amanero::listen_pin_changes(CMD_CHANNEL.sender(), amanero)));
            });
        },
    );

    // receive commands on core0
    let executor0 = EXECUTOR0.init(Executor::new());
    executor0.run(|spawner| {
        unwrap!(spawner.spawn(tick_display()));
        unwrap!(spawner.spawn(process_commands(dac, rsplayer, res.out, res.display, flash)));
        unwrap!(spawner.spawn(usb_task(usb_device)));
        unwrap!(spawner.spawn(usb::listen_usb_commands(CMD_CHANNEL.sender(), usb_rx)));
    });
}

type MyUsbDriver = Driver<'static, USB>;
type MyUsbDevice = UsbDevice<'static, MyUsbDriver>;

#[embassy_executor::task]
async fn usb_task(mut usb: MyUsbDevice) {
    usb.run().await
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
pub async fn tick_display() {
    loop {
        let is_power_on = POWER_ON.load(core::sync::atomic::Ordering::Relaxed);
        if is_power_on {
            if let Some(disp) = DISPLAY.lock().await.as_mut() {
                disp.tick();
            }
        }
        Timer::after_millis(50).await;
    }
}

#[embassy_executor::task]
pub async fn process_commands(
    mut dac: Akm44xxDac,
    mut rsplayer: RsPlayer,
    out_resources: OutputPins,
    display_resources: DisplayResources,
    mut flash: flash::Storage,
) {
    let mut pwr_psu_relay = Output::new(out_resources.pin1, Level::Low);
    let mut mute_out_relay = Output::new(out_resources.pin0, Level::Low);
    let mut i2s_signal_select = Output::new(out_resources.pin6, Level::High);
    let mut last_sample_rate = None;
    
    // Backup resources for re-initialization if needed
    let mut display_res = Some(display_resources);

    {
        let res = display_res.take().unwrap();
        let mut d_lock = DISPLAY.lock().await;
        d_lock.replace(OledDisplay::new(res));
    }
    
    if let Some(disp) = DISPLAY.lock().await.as_mut() {
        disp.draw_powered_off();
    }
    mute_out_relay.set_low();
    let mut current_volume = flash.load_volume();
    let saved_display_mode = flash.load_display_mode();
    let mut display_mode = DisplayMode::from(saved_display_mode);
    if let Some(disp) = DISPLAY.lock().await.as_mut() {
        disp.set_display_mode(display_mode);
    }
    
    let mut current_playback_mode = PlaybackMode::Sequential;
    loop {
        let cmd = CMD_CHANNEL.receive().await;
        let is_power_on = POWER_ON.load(core::sync::atomic::Ordering::SeqCst);
        if cmd != Command::TogglePower && !is_power_on {
            info!("Power is off, ignoring command");
            continue;
        }
        let input = flash.load_input();
        let stored_filter = flash.load_filter_type();
        let mut current_input = if input == 0 { "OPT" } else { "USB" };
        let mut current_filter = FilterType::from(stored_filter).as_str();

        match cmd {
            Command::ToggleDisplayMode => {
                display_mode = match display_mode {
                    DisplayMode::Normal => DisplayMode::VuMeter,
                    DisplayMode::VuMeter => DisplayMode::BigInfo,
                    DisplayMode::BigInfo => DisplayMode::Normal,
                };
                flash.save_display_mode(display_mode as u8);
                
                let mut disp_lock = DISPLAY.lock().await;
                let d = disp_lock.as_mut().unwrap();
                d.set_display_mode(display_mode);

                d.draw_background();
                d.draw_layout_lines();

                match display_mode {
                    DisplayMode::Normal => {
                        d.draw_header_status(current_input, current_filter);
                        d.redraw_footer();
                        if current_input == "OPT" {
                            d.draw_large_volume(current_volume);
                        } else {
                            d.redraw_track_info();
                            d.draw_progress_bar("00:00", "00:00", 0.0);
                        }
                    }
                    DisplayMode::VuMeter => {
                        d.draw_fullscreen_vu_labels();
                    }
                    DisplayMode::BigInfo => {
                        d.redraw_track_info();
                        d.draw_volume(current_volume);
                        d.draw_playback_mode(current_playback_mode);
                        d.redraw_footer();
                    }
                }
            }
            Command::TogglePower => {
                let is_power_on = POWER_ON.load(core::sync::atomic::Ordering::SeqCst);
                if !is_power_on {
                    pwr_psu_relay.set_high();
                    Timer::after_millis(1000).await;

                    POWER_ON.store(true, core::sync::atomic::Ordering::Relaxed);
                    let mut disp_lock = DISPLAY.lock().await;
                    let disp = disp_lock.as_mut().unwrap();

                    let stored_sound = flash.load_sound_setting();
                    let stored_volume = flash.load_volume();
                    current_volume = stored_volume;
                    dac.initialize(stored_filter, stored_sound).await;
                    dac.set_volume(stored_volume).await;
                    debug!("Stored input: {}", input);
                    if input == 0 {
                        i2s_signal_select.set_low();
                        dac.dsd_pcm(SampleRate::Pcm441).await;
                    } else {
                        i2s_signal_select.set_high();
                        amanero::REFRESH_SAMPLE_RATE.signal(());
                    }
                    disp.turn_on_backlight();
                    disp.draw_background();
                    disp.draw_layout_lines();
                    disp.draw_header_status(current_input, current_filter);
                    disp.draw_playback_mode(current_playback_mode);
                    disp.draw_volume(stored_volume);
                    match display_mode {
                        DisplayMode::Normal => {
                            if input == 0 {
                                disp.draw_large_volume(stored_volume);
                            } else {
                                disp.redraw_track_info();
                                disp.draw_progress_bar("00:00", "00:00", 0.0);
                            }
                        }
                        DisplayMode::VuMeter => {
                             disp.draw_fullscreen_vu_labels();
                        }
                        DisplayMode::BigInfo => {
                            disp.redraw_track_info();
                            disp.redraw_footer();
                        }
                    }
                    disp.draw_footer("", "", "");
                    // rsplayer.send_command("QueryCurrentPlayerInfo").await;
                } else {
                    debug!("Powering off");
                    mute_out_relay.set_low();
                    
                    if let Some(disp) = DISPLAY.lock().await.as_mut() {
                        disp.draw_powered_off();
                    }
                    Timer::after_millis(200).await;

                    pwr_psu_relay.set_low();
                    POWER_ON.store(false, core::sync::atomic::Ordering::SeqCst);
                    last_sample_rate = None;
                    debug!("Powered off");
                }
            }
            Command::VolumeUp => {
                info!("got VolumeUp");
                let new_val = dac.volume_up().await;
                flash.save_volume(new_val);
                current_volume = new_val;
                {
                    let mut d = DISPLAY.lock().await;
                    if let Some(disp) = d.as_mut() {
                        disp.draw_volume(new_val);
                        if current_input == "OPT" && display_mode == DisplayMode::Normal {
                            disp.draw_large_volume(new_val);
                        }
                    }
                }
                rsplayer.send_current_volume(new_val).await;
            }
            Command::VolumeDown => {
                info!("got VolumeDown");
                let new_val = dac.volume_down().await;
                flash.save_volume(new_val);
                current_volume = new_val;
                {
                    let mut d = DISPLAY.lock().await;
                    if let Some(disp) = d.as_mut() {
                        disp.draw_volume(new_val);
                        if current_input == "OPT" && display_mode == DisplayMode::Normal {
                            disp.draw_large_volume(new_val);
                        }
                    }
                }
                rsplayer.send_current_volume(new_val).await;
            }
            Command::SetVolume(vol) => {
                info!("Received SetVolume({})", vol);
                dac.set_volume(vol).await;
                flash.save_volume(vol);
                current_volume = vol;
                {
                    let mut d = DISPLAY.lock().await;
                    if let Some(disp) = d.as_mut() {
                        disp.draw_volume(vol);
                        if current_input == "OPT" && display_mode == DisplayMode::Normal {
                            disp.draw_large_volume(vol);
                        }
                    }
                }
                rsplayer.send_current_volume(vol).await;
            }
            Command::ToggleRandomPlay => {
                info!("got CyclePlaybackMode");
                rsplayer.send_command("CyclePlaybackMode").await;
            }
            Command::ToggleInput => {
                mute_out_relay.set_low();
                Timer::after_millis(100).await;
                last_sample_rate = None;
                // select optical or coaxial input
                if i2s_signal_select.is_set_low() {
                    info!("Input signal relay set high");
                    i2s_signal_select.set_high();
                    flash.save_input(1);
                    current_input = "USB";
                    amanero::REFRESH_SAMPLE_RATE.signal(());
                    let mut d_lock = DISPLAY.lock().await;
                    let disp = d_lock.as_mut().unwrap();
                    disp.clear_main_area();
                    disp.draw_header_status(current_input, current_filter);
                    disp.draw_playback_mode(current_playback_mode);
                    if display_mode == DisplayMode::Normal {
                        disp.redraw_track_info();
                        disp.draw_progress_bar("00:00", "00:00", 0.0);
                    }
                }
                // select i2s input
                else {
                    info!("Input signal relay set low");
                    i2s_signal_select.set_low();
                    flash.save_input(0);
                    dac.dsd_pcm(SampleRate::Pcm441).await;
                    current_input = "OPT";
                    let mut d_lock = DISPLAY.lock().await;
                    let disp = d_lock.as_mut().unwrap();
                    disp.clear_main_area();
                    disp.draw_header_status(current_input, current_filter);
                    disp.draw_playback_mode(current_playback_mode);
                    if display_mode == DisplayMode::Normal {
                        disp.draw_large_volume(current_volume);
                    } else if display_mode == DisplayMode::BigInfo {
                        disp.draw_volume(current_volume);
                    }
                }
                Timer::after_millis(100).await;
                mute_out_relay.set_high();
            }

            Command::Next => {
                rsplayer.send_command("Next").await;
            }
            Command::Prev => {
                rsplayer.send_command("Prev").await;
            }
            Command::TogglePlay => {
                rsplayer.send_command("TogglePlay").await;
            }
            Command::NextDacFilterType => {
                info!("got NextDacFilterType");
                let val = dac.next_filter().await;
                flash.save_filter_type(val);
                current_filter = FilterType::from(val).as_str();
                if let Some(disp) = DISPLAY.lock().await.as_mut() {
                    disp.draw_header_status(current_input, current_filter);
                }
            }
            Command::NextDacSoundSetting => {
                info!("got NextDacSoundSetting");
                let val = dac.next_sound_setting().await;
                flash.save_sound_setting(val);
            }

            Command::QueryCurrentVolume => {
                let vol = flash.load_volume();
                rsplayer.send_current_volume(vol).await;
            }
            Command::UpdateSampleRate(rate) => {
                if input != 1 {
                    continue;
                }
                debug!("Sample rate command: {}", rate);
                if last_sample_rate == Some(rate) {
                    continue;
                }
                mute_out_relay.set_low();
                Timer::after_millis(50).await;
                dac.dsd_pcm(rate).await;
                Timer::after_millis(50).await;
                mute_out_relay.set_high();
                last_sample_rate = Some(rate);
                let (format, freq, bit_depth) = rate.to_str();
                if let Some(disp) = DISPLAY.lock().await.as_mut() {
                    disp.draw_footer(format, freq, bit_depth);
                }
            }
            Command::UpdateTrackInfo {
                title,
                artist,
                album,
            } => {
                if display_mode != DisplayMode::VuMeter && current_input != "OPT" {
                    if let Some(disp) = DISPLAY.lock().await.as_mut() {
                        disp.draw_track_info(&title, &artist, &album);
                    }
                }
            }
            Command::UpdateProgress {
                current,
                total,
                percent,
            } => {
                if display_mode == DisplayMode::Normal && current_input != "OPT" {
                    if let Some(disp) = DISPLAY.lock().await.as_mut() {
                        disp.draw_progress_bar(&current, &total, percent as f32 / 100.0);
                    }
                }
            }
            Command::UpdatePlaybackMode(mode) => {
                debug!("UpdatePlaybackMode received: {:?}", mode);
                current_playback_mode = mode;
                if let Some(disp) = DISPLAY.lock().await.as_mut() {
                    disp.draw_playback_mode(mode);
                }
            }
            Command::UpdateVU { left, right } => {
                let mut disp_lock = DISPLAY.lock().await;
                if let Some(d) = disp_lock.as_mut() {
                    if display_mode == DisplayMode::VuMeter {
                        d.draw_fullscreen_vu_meter(left, right, current_volume);
                    } else {
                        d.draw_vu_meter(left, right, current_volume);
                    }
                }
            }
            _ => {}
        }
    }
}
