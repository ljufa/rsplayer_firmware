# rsplayer_firmware

This repository contains the firmware for the hardware controller of the [rsplayer](https://github.com/ljufa/rsplayer) music player system. The firmware is designed to run on a Raspberry Pi Pico (RP2040 microcontroller) and serves as the main user interface and control hub for the system.

It works in conjunction with the main `rsplayer` application (running on a separate host, like a Raspberry Pi) and the custom hardware defined in the `rsplayer_hardware` project.

## Features

*   **System Control:** Communicates with the main `rsplayer` application via UART to send commands like Play, Pause, Next, Previous, and Power Off.
*   **Power Management:** Controls power relays for the entire system, including the host Raspberry Pi and the main Power Supply Unit (PSU).
*   **User Interface:**
    *   Displays system status, volume levels, and input source on a 128x64 ST7920-based LCD.
    *   Automatically dims and turns off the display backlight after a period of inactivity.
*   **Input Handling:**
    *   **IR Remote:** Responds to commands from a standard NEC-protocol IR remote.
    *   **Rotary Encoder:** Allows for precise volume adjustment.
    *   **Rotary Encoder Button:**
        *   Short Press: Toggles Play/Pause.
        *   Long Press (>5s): Toggles system power.
*   **DAC Control:**
    *   Directly manages an I2C-connected DAC.
    *   Software-based volume control.
    *   Switches between DSD and PCM modes.
    *   Cycles through various DAC digital filters and sound settings.
*   **Input Source Selection:** Toggles between the internal I2S signal from the host and an external optical/coaxial input.
*   **Persistent Settings:** Saves the last used volume and input source to the microcontroller's flash memory, restoring them on startup.

## Demo
[![Watch the video](https://img.youtube.com/vi/8EiTv39dqec/maxresdefault.jpg)](https://youtube.com/shorts/8EiTv39dqec)
[![Watch the video](https://img.youtube.com/vi/vZ4sb1H7nrk/maxresdefault.jpg)](https://youtube.com/shorts/vZ4sb1H7nrk)

## Hardware

This firmware is specifically tailored for the custom hardware designed in the `rsplayer_hardware` project. The key components are:

*   **Microcontroller:** Raspberry Pi Pico (RP2040)
*   **Display:** ST7920-based 128x64 monochrome LCD.
*   **DAC:** An I2C-controlled Digital-to-Analog Converter.
*   **Inputs:**
    *   Standard IR receiver (e.g., TSOP38238).
    *   Rotary encoder with an integrated push-button.
*   **Communication:** UART interface for connecting to the host system.

## Related Projects

*   **[rsplayer](https://github.com/ljufa/rsplayer):** The core music player application that runs on a Linux host (e.g., Raspberry Pi).
*   **[rsplayer_hardware](https://github.com/ljufa/rsplayer_hardware):** The repository containing the KiCad schematics and PCB layout files for the hardware.

## Building and Flashing

The firmware is built using the `embassy` async framework for embedded Rust.

### Prerequisites

1.  **Rust Toolchain:** Install Rust using [rustup](https://rustup.rs/).
2.  **Target:** Add the required target for the RP2040:
    ```sh
    rustup target add thumbv6m-none-eabi
    ```
3.  **probe-rs:** Install the flashing and debugging tool:
    ```sh
    cargo install probe-rs
    ```
4.  **Debug Probe:** You will need a debug probe compatible with the RP2040, such as a second Raspberry Pi Pico running the [Picoprobe firmware](https://github.com/raspberrypi/picoprobe).

### Building

Clone the repository and build the firmware in release mode:

```sh
git clone https://github.com/ljufa/rsplayer_firmware.git
cd rsplayer_firmware
cargo build --release
```

### Flashing

Connect the debug probe to your development machine and the target hardware. Then, use `probe-rs` to flash the firmware.

```sh
probe-rs run --chip RP2040 --connect-under-reset
```

The command specified in the `.cargo/config.toml` file will be used, which simplifies the process.
