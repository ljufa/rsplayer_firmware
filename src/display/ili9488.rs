#![allow(unused_imports)]

use core::fmt::Write;
use embedded_graphics::{
    pixelcolor::Rgb666,
    prelude::*,
    primitives::Rectangle,
    text::{renderer::TextRenderer, Alignment, Baseline, Text, TextStyleBuilder},
    Drawable,
};
use heapless::String;
use u8g2_fonts::{
    fonts,
    types::{FontColor, HorizontalAlignment, VerticalPosition},
    FontRenderer, U8g2TextStyle,
};

// UI Constants (Merged from ui/src/lib.rs)
pub const COL_BG_BASE: Rgb666 = Rgb666::BLACK;
pub const COL_VU_MAX: Rgb666 = Rgb666::RED; 
pub const COL_TEXT: Rgb666 = Rgb666::new(63, 63, 0); // Yellow
pub const COL_1: Rgb666 = Rgb666::new(0, 63, 63); // Cyan


const BAR_Y: i32 = 215;
const BAR_WIDTH: u32 = 400;
const BAR_HEIGHT: u32 = 16;
const BAR_X: i32 = (480 - BAR_WIDTH as i32) / 2;
const TIME_Y: i32 = 255;

const VU_MARGIN_X: i32 = 12;
const VU_WIDTH: u32 = 10;
const VU_MAX_HEIGHT: u32 = 202;
const VU_TOP_Y: i32 = 62;

// Shared buffer for line drawing to save RAM
// Note: We use a static mutable buffer. Ensure single-threaded access (tick is sequential).
static mut LINE_BUFFER: [Rgb666; 480 * 50] = [Rgb666::new(0, 0, 0); 480 * 50];

pub struct LineBuffer<'a> {
    buffer: &'a mut [Rgb666],
    width: u32,
    height: u32,
}

impl<'a> LineBuffer<'a> {
    pub fn new(buffer: &'a mut [Rgb666], width: u32, height: u32) -> Self {
        Self {
            buffer,
            width,
            height,
        }
    }
}

impl<'a> OriginDimensions for LineBuffer<'a> {
    fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }
}

impl<'a> DrawTarget for LineBuffer<'a> {
    type Color = Rgb666;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(pt, color) in pixels {
            if pt.x >= 0 && pt.x < self.width as i32 && pt.y >= 0 && pt.y < self.height as i32 {
                let idx = (pt.y as usize * self.width as usize) + pt.x as usize;
                if idx < self.buffer.len() {
                    self.buffer[idx] = color;
                }
            }
        }
        Ok(())
    }
}

pub struct PlayerDisplay<D> {
    pub display: D,
    track_artist: String<64>,
    track_title: String<64>,
    track_album: String<64>,
    scroll_tick: i32,
    last_total_time: String<16>,
    vu_mode_fullscreen: bool,
}

impl<D> PlayerDisplay<D>
where
    D: DrawTarget<Color = Rgb666> + OriginDimensions,
    D::Error: core::fmt::Debug,
{
    pub fn new(display: D) -> Self {
        Self {
            display,
            track_title: String::new(),
            track_artist: String::new(),
            track_album: String::new(),
            scroll_tick: 0,
            last_total_time: String::new(),
            vu_mode_fullscreen: false,
        }
    }

    pub fn set_fullscreen_vu_mode(&mut self, enable: bool) {
        self.vu_mode_fullscreen = enable;
    }

    pub fn draw_background(&mut self) {
        Rectangle::new(Point::new(0, 0), self.display.size())
            .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(
                COL_BG_BASE,
            ))
            .draw(&mut self.display)
            .ok();
    }

    pub fn draw_layout_lines(&mut self) {
        let style_line =
            embedded_graphics::primitives::PrimitiveStyle::with_stroke(Rgb666::WHITE, 2);

        embedded_graphics::primitives::Line::new(Point::new(0, 60), Point::new(479, 60))
            .into_styled(style_line)
            .draw(&mut self.display)
            .ok();

        embedded_graphics::primitives::Line::new(Point::new(0, 265), Point::new(479, 265))
            .into_styled(style_line)
            .draw(&mut self.display)
            .ok();
    }

    pub fn draw_header_status(&mut self, input: &str, filter: &str) {
        Rectangle::new(Point::new(5, 25), Size::new(340, 30))
            .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(
                COL_BG_BASE,
            ))
            .draw(&mut self.display)
            .ok();

        let style_label = U8g2TextStyle::new(fonts::u8g2_font_helvB18_tf, Rgb666::WHITE);
        let style_value = U8g2TextStyle::new(fonts::u8g2_font_helvB18_tf, COL_1);

        Text::new("IN:", Point::new(5, 45), style_label.clone())
            .draw(&mut self.display)
            .ok();
        Text::new(input, Point::new(40, 45), style_value.clone())
            .draw(&mut self.display)
            .ok();

        Text::new("| FLT:", Point::new(105, 45), style_label)
            .draw(&mut self.display)
            .ok();
        Text::new(filter, Point::new(170, 45), style_value)
            .draw(&mut self.display)
            .ok();
    }

    pub fn draw_volume(&mut self, vol_db: u8) {
        let x = 345;
        let y = 25;
        let width = 130;
        let height = 30;

        let buffer_slice = unsafe { &mut LINE_BUFFER[..(width * height) as usize] };
        buffer_slice.fill(COL_BG_BASE);

        let mut target = LineBuffer::new(buffer_slice, width, height);

        let style_label = U8g2TextStyle::new(fonts::u8g2_font_helvB18_tf, Rgb666::WHITE);
        let style_value = U8g2TextStyle::new(fonts::u8g2_font_helvB18_tf, COL_1);

        let text_style_right = TextStyleBuilder::new()
            .alignment(Alignment::Right)
            .baseline(Baseline::Alphabetic)
            .build();

        let mut vol_str = String::<32>::new(); 
        write!(&mut vol_str, "{}", vol_db).unwrap();

        Text::new("| VOL:", Point::new(0, 20), style_label)
            .draw(&mut target)
            .ok();

        Text::with_text_style(
            &vol_str,
            Point::new(width as i32 - 5, 20),
            style_value,
            text_style_right,
        )
        .draw(&mut target)
        .ok();

        self.display.fill_contiguous(
            &Rectangle::new(Point::new(x, y), Size::new(width, height)),
            target.buffer.iter().cloned()
        ).ok();
    }

    pub fn tick(&mut self) {
        self.scroll_tick += 2;
        if !self.vu_mode_fullscreen {
            self.draw_track_info_internal();
        }
    }

    pub fn draw_track_info(&mut self, title: &str, artist: &str, album: &str) {
        self.track_title.clear();
        self.track_title.push_str(title).ok();
        self.track_artist.clear();
        self.track_artist.push_str(artist).ok();
        self.track_album.clear();
        self.track_album.push_str(album).ok();
        self.scroll_tick = 0;
        self.draw_track_info_internal();
    }

    pub fn redraw_track_info(&mut self) {
        self.scroll_tick = 0;
        self.draw_track_info_internal();
    }

    fn draw_track_info_internal(&mut self) {
        let style_song = U8g2TextStyle::new(fonts::u8g2_font_helvB24_tf, Rgb666::WHITE);
        let style_artist = U8g2TextStyle::new(fonts::u8g2_font_helvB18_tf, COL_1);
        let style_album = U8g2TextStyle::new(fonts::u8g2_font_helvB18_tf, COL_TEXT);

        let content_width = 480 - (VU_MARGIN_X * 2);
        let center_x = content_width / 2;

        let mut draw_buffered_line =
            |style: U8g2TextStyle<Rgb666>, text: &str, y: i32, height: u32| {
                let width = style
                    .measure_string(text, Point::zero(), Baseline::Middle)
                    .bounding_box
                    .size
                    .width as i32;

                let buffer_slice = unsafe { &mut LINE_BUFFER[..(content_width as usize * height as usize)] };
                buffer_slice.fill(COL_BG_BASE);

                let mut target = LineBuffer::new(buffer_slice, content_width as u32, height);
                let local_y = height as i32 / 2;

                if width <= content_width {
                    Text::with_text_style(
                        text,
                        Point::new(center_x, local_y),
                        style,
                        TextStyleBuilder::new()
                            .alignment(Alignment::Center)
                            .baseline(Baseline::Middle)
                            .build(),
                    )
                    .draw(&mut target)
                    .ok();
                } else {
                    let gap = 60;
                    let cycle = width + gap;
                    let offset = self.scroll_tick % cycle;
                    let x = 10 - offset;

                    Text::with_text_style(
                        text,
                        Point::new(x, local_y),
                        style.clone(),
                        TextStyleBuilder::new()
                            .alignment(Alignment::Left)
                            .baseline(Baseline::Middle)
                            .build(),
                    )
                    .draw(&mut target)
                    .ok();

                    if x + width < content_width {
                        Text::with_text_style(
                            text,
                            Point::new(x + cycle, local_y),
                            style,
                            TextStyleBuilder::new()
                                .alignment(Alignment::Left)
                                .baseline(Baseline::Middle)
                                .build(),
                        )
                        .draw(&mut target)
                        .ok();
                    }
                }

                let screen_top = y - local_y;
                self.display.fill_contiguous(
                    &Rectangle::new(Point::new(VU_MARGIN_X, screen_top), Size::new(content_width as u32, height)),
                    target.buffer.iter().cloned()
                ).ok();
            };

        draw_buffered_line(style_song, &self.track_title, 90, 50);
        draw_buffered_line(style_artist, &self.track_artist, 135, 35);
        draw_buffered_line(style_album, &self.track_album, 180, 45);
    }

    pub fn draw_vu_meter(&mut self, left: u8, right: u8, volume: u8) {
        let style_bg = embedded_graphics::primitives::PrimitiveStyle::with_fill(COL_BG_BASE);

        let scale = volume as f32 / 255.0;
        let left_scaled = left as f32 * scale;
        let right_scaled = right as f32 * scale;

        let h_left = (left_scaled / 255.0 * VU_MAX_HEIGHT as f32) as u32;
        let h_right = (right_scaled / 255.0 * VU_MAX_HEIGHT as f32) as u32;

        let green_limit = (VU_MAX_HEIGHT as f32 * 0.7) as u32;
        let orange_limit = (VU_MAX_HEIGHT as f32 * 0.9) as u32;

        let mut draw_vert_bar = |x: i32, current_height: u32| {
            let empty_h = VU_MAX_HEIGHT - current_height;
            if empty_h > 0 {
                Rectangle::new(Point::new(x, VU_TOP_Y), Size::new(VU_WIDTH, empty_h))
                    .into_styled(style_bg)
                    .draw(&mut self.display)
                    .ok();
            }

            let bottom_y = VU_TOP_Y + VU_MAX_HEIGHT as i32;

            let cyan_h = current_height.min(green_limit);
            if cyan_h > 0 {
                Rectangle::new(
                    Point::new(x, bottom_y - cyan_h as i32),
                    Size::new(VU_WIDTH, cyan_h),
                )
                .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(COL_1))
                .draw(&mut self.display)
                .ok();
            }

            if current_height > green_limit {
                let yellow_h = current_height.min(orange_limit) - green_limit;
                let yellow_bottom = bottom_y - green_limit as i32;
                Rectangle::new(
                    Point::new(x, yellow_bottom - yellow_h as i32),
                    Size::new(VU_WIDTH, yellow_h),
                )
                .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(COL_TEXT))
                .draw(&mut self.display)
                .ok();
            }

            if current_height > orange_limit {
                let green_h = current_height - orange_limit;
                let green_bottom = bottom_y - orange_limit as i32;
                Rectangle::new(
                    Point::new(x, green_bottom - green_h as i32),
                    Size::new(VU_WIDTH, green_h),
                )
                .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(COL_VU_MAX))
                .draw(&mut self.display)
                .ok();
            }
        };

        draw_vert_bar(0, h_left);
        draw_vert_bar(480 - VU_WIDTH as i32, h_right);
    }

    pub fn draw_bar(&mut self, progress: f32) {
        let buffer_slice = unsafe { &mut LINE_BUFFER[..(BAR_WIDTH * BAR_HEIGHT) as usize] };
        buffer_slice.fill(COL_BG_BASE); // Default background

        let mut target = LineBuffer::new(buffer_slice, BAR_WIDTH, BAR_HEIGHT);

        let style_progress_fill = embedded_graphics::primitives::PrimitiveStyle::with_fill(COL_1);
        let style_progress_bg = embedded_graphics::primitives::PrimitiveStyle::with_fill(COL_BG_BASE);
        let style_border = embedded_graphics::primitives::PrimitiveStyleBuilder::new()
            .stroke_color(Rgb666::WHITE)
            .stroke_width(2)
            .build();

        let fill_width = (BAR_WIDTH as f32 * progress) as u32;
        let fill_width = fill_width.min(BAR_WIDTH);

        // Draw into buffer (relative coordinates 0,0)
        
        // 1. Fill
        if fill_width > 0 {
            Rectangle::new(Point::new(0, 0), Size::new(fill_width, BAR_HEIGHT))
                .into_styled(style_progress_fill)
                .draw(&mut target)
                .ok();
        }

        // 2. Empty part
        let empty_width = BAR_WIDTH - fill_width;
        if empty_width > 0 {
            Rectangle::new(
                Point::new(fill_width as i32, 0),
                Size::new(empty_width, BAR_HEIGHT),
            )
            .into_styled(style_progress_bg)
            .draw(&mut target)
            .ok();
        }

        // 3. Border
        Rectangle::new(Point::new(0, 0), Size::new(BAR_WIDTH, BAR_HEIGHT))
            .into_styled(style_border)
            .draw(&mut target)
            .ok();

        // Blit to screen
        self.display.fill_contiguous(
            &Rectangle::new(Point::new(BAR_X, BAR_Y), Size::new(BAR_WIDTH, BAR_HEIGHT)),
            target.buffer.iter().cloned()
        ).ok();
    }

    pub fn draw_current_time(&mut self, curr_time: &str) {
        let width = 100;
        let height = 25;
        let x = BAR_X;
        let y = TIME_Y - 20;

        let buffer_slice = unsafe { &mut LINE_BUFFER[..(width * height) as usize] };
        buffer_slice.fill(COL_BG_BASE);

        let mut target = LineBuffer::new(buffer_slice, width, height);
        let font_time = FontRenderer::new::<fonts::u8g2_font_helvB18_tf>();

        font_time
            .render_aligned(
                curr_time,
                Point::new(0, 20),
                VerticalPosition::Baseline,
                HorizontalAlignment::Left,
                FontColor::Transparent(Rgb666::WHITE),
                &mut target,
            )
            .unwrap();

        self.display.fill_contiguous(
            &Rectangle::new(Point::new(x, y), Size::new(width, height)),
            target.buffer.iter().cloned()
        ).ok();
    }

    pub fn draw_total_time(&mut self, total_time: &str) {
        let width = 100;
        let height = 25;
        let x = BAR_X + BAR_WIDTH as i32 - 100;
        let y = TIME_Y - 20;

        let buffer_slice = unsafe { &mut LINE_BUFFER[..(width * height) as usize] };
        buffer_slice.fill(COL_BG_BASE);

        let mut target = LineBuffer::new(buffer_slice, width, height);
        let font_time = FontRenderer::new::<fonts::u8g2_font_helvB18_tf>();

        font_time
            .render_aligned(
                total_time,
                Point::new(width as i32, 20),
                VerticalPosition::Baseline,
                HorizontalAlignment::Right,
                FontColor::Transparent(Rgb666::WHITE),
                &mut target,
            )
            .unwrap();

        self.display.fill_contiguous(
            &Rectangle::new(Point::new(x, y), Size::new(width, height)),
            target.buffer.iter().cloned()
        ).ok();
    }

    pub fn draw_progress_bar(&mut self, curr_time: &str, total_time: &str, progress: f32) {
        self.draw_bar(progress);
        self.draw_current_time(curr_time);
        if self.last_total_time != total_time {
            self.draw_total_time(total_time);
            self.last_total_time.clear();
            self.last_total_time.push_str(total_time).ok();
        }
    }

    pub fn draw_footer(&mut self, format: &str, freq: &str, bit_depth: &str) {
        Rectangle::new(Point::new(0, 271), Size::new(480, 49))
            .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(
                COL_BG_BASE,
            ))
            .draw(&mut self.display)
            .ok();

        let style_footer = U8g2TextStyle::new(fonts::u8g2_font_helvB24_tf, COL_1);
        let text_style_center = TextStyleBuilder::new()
            .alignment(Alignment::Center)
            .baseline(Baseline::Middle)
            .build();

        let footer_y = 295;
        let section_width = 480 / 3;

        Text::with_text_style(
            format,
            Point::new(section_width / 2, footer_y),
            style_footer.clone(),
            text_style_center,
        )
        .draw(&mut self.display)
        .ok();

        Text::with_text_style(
            freq,
            Point::new(section_width + section_width / 2, footer_y),
            style_footer.clone(),
            text_style_center,
        )
        .draw(&mut self.display)
        .ok();

        Text::with_text_style(
            bit_depth,
            Point::new(section_width * 2 + section_width / 2, footer_y),
            style_footer,
            text_style_center,
        )
        .draw(&mut self.display)
        .ok();
    }

    pub fn draw_powered_off(&mut self) {
        let font_huge = FontRenderer::new::<fonts::u8g2_font_fub42_tf>();
        
        Rectangle::new(Point::new(0, 0), self.display.size())
             .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(COL_BG_BASE))
             .draw(&mut self.display)
             .ok();

        _ = font_huge.render_aligned(
            "Off",
            self.display.bounding_box().center(),
            VerticalPosition::Center,
            u8g2_fonts::types::HorizontalAlignment::Center,
            FontColor::Transparent(Rgb666::WHITE),
            &mut self.display,
        );
    }

    pub fn clear_main_area(&mut self) {
        Rectangle::new(Point::new(0, 62), Size::new(480, 203))
            .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(
                COL_BG_BASE,
            ))
            .draw(&mut self.display)
            .ok();
    }

    pub fn draw_fullscreen_vu_meter(&mut self, left: u8, right: u8, volume: u8) {
        let max_width: u32 = 400;
        let bar_height: u32 = 40;
        let start_x: i32 = (480 - max_width as i32) / 2;
        let l_y: i32 = 100;
        let r_y: i32 = 180;

        let scale = volume as f32 / 255.0;
        let left_scaled = left as f32 * scale;
        let right_scaled = right as f32 * scale;

        let w_left = (left_scaled / 255.0 * max_width as f32) as u32;
        let w_right = (right_scaled / 255.0 * max_width as f32) as u32;

        let green_limit = (max_width as f32 * 0.7) as u32;
        let orange_limit = (max_width as f32 * 0.9) as u32;

        // Use a single buffer for drawing one bar. 400x40 = 16000 pixels.
        // LINE_BUFFER is 480x50 = 24000 pixels, so it fits.
        
        let mut draw_horiz_bar = |y: i32, current_width: u32| {
            let buffer_slice = unsafe { &mut LINE_BUFFER[..(max_width * bar_height) as usize] };
            // Clear background
            buffer_slice.fill(Rgb666::CSS_DIM_GRAY); 

            let mut target = LineBuffer::new(buffer_slice, max_width, bar_height);

            let cyan_w = current_width.min(green_limit);
            if cyan_w > 0 {
                Rectangle::new(Point::new(0, 0), Size::new(cyan_w, bar_height))
                    .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(COL_1))
                    .draw(&mut target)
                    .ok();
            }

            if current_width > green_limit {
                let yellow_w = current_width.min(orange_limit) - green_limit;
                Rectangle::new(Point::new(green_limit as i32, 0), Size::new(yellow_w, bar_height))
                    .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(COL_TEXT))
                    .draw(&mut target)
                    .ok();
            }

            if current_width > orange_limit {
                let green_w = current_width - orange_limit;
                Rectangle::new(Point::new(orange_limit as i32, 0), Size::new(green_w, bar_height))
                    .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(COL_VU_MAX))
                    .draw(&mut target)
                    .ok();
            }

            // Blit the whole bar at once
            self.display.fill_contiguous(
                &Rectangle::new(Point::new(start_x, y), Size::new(max_width, bar_height)),
                target.buffer.iter().cloned()
            ).ok();
        };

        draw_horiz_bar(l_y, w_left);
        draw_horiz_bar(r_y, w_right);
    }
    
    pub fn draw_fullscreen_vu_labels(&mut self) {
        let max_width: u32 = 400;
        let start_x: i32 = (480 - max_width as i32) / 2;
        let l_y: i32 = 100;
        let r_y: i32 = 180;
        
        let style_label = U8g2TextStyle::new(fonts::u8g2_font_helvB24_tf, Rgb666::WHITE);
        Text::new("L", Point::new(start_x - 30, l_y + 30), style_label.clone())
            .draw(&mut self.display)
            .ok();
        Text::new("R", Point::new(start_x - 30, r_y + 30), style_label)
            .draw(&mut self.display)
            .ok();
    }

    pub fn draw_large_volume(&mut self, vol: u8) {
        let width = 200;
        let height = 60;
        
        let buffer_slice = unsafe { &mut LINE_BUFFER[..(width * height) as usize] };
        buffer_slice.fill(COL_BG_BASE);

        let mut target = LineBuffer::new(buffer_slice, width as u32, height as u32);

        let style = U8g2TextStyle::new(fonts::u8g2_font_fub42_tf, COL_1);
        
        let mut vol_str = String::<16>::new();
        write!(&mut vol_str, "{}", vol).unwrap();

        Text::with_text_style(
            &vol_str,
            Point::new(width / 2, height / 2 + 10),
            style,
            TextStyleBuilder::new().alignment(Alignment::Center).baseline(Baseline::Middle).build(),
        )
        .draw(&mut target)
        .ok();

        let screen_x = (480 - width) / 2;
        let screen_y = 62 + (203 - height) / 2;

        self.display.fill_contiguous(
            &Rectangle::new(Point::new(screen_x, screen_y), Size::new(width as u32, height as u32)),
            target.buffer.iter().cloned()
        ).ok();
    }
}

// Hardware Specifics (Targeted for embedded)
#[cfg(target_arch = "arm")]
mod hardware {
    use super::*;
    use embassy_rp::gpio::{Level, Output};
    use embassy_rp::spi;
    use embassy_time::{Delay, Instant};
    use embedded_hal_bus::spi::ExclusiveDevice;
    use crate::DisplayResources;
    use {defmt_rtt as _};
    use display_interface_spi::SPIInterface;
    use ili9488_rs::{Ili9488, Orientation, Rgb666Mode};

    struct DummyCs;
    impl embedded_hal_1::digital::OutputPin for DummyCs {
        fn set_low(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
        fn set_high(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }
    impl embedded_hal_1::digital::ErrorType for DummyCs {
        type Error = core::convert::Infallible;
    }

    type SpiDeviceDisp =
        ExclusiveDevice<spi::Spi<'static, embassy_rp::peripherals::SPI0, spi::Async>, DummyCs, Delay>;

    type Display = Ili9488<SPIInterface<SpiDeviceDisp, Output<'static>>, Output<'static>, Rgb666Mode>;

    struct DisplayWrapper(Display);

    impl OriginDimensions for DisplayWrapper {
        fn size(&self) -> Size {
            Size::new(480, 320)
        }
    }

    impl DrawTarget for DisplayWrapper {
        type Color = Rgb666;
        type Error = <Display as DrawTarget>::Error;

        fn draw_iter<I>(&mut self, pixels: I) -> Result<(), <Self as DrawTarget>::Error>
        where
            I: IntoIterator<Item = embedded_graphics::Pixel<Self::Color>>,
        {
            self.0.draw_iter(pixels)
        }

        fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
        where
            I: IntoIterator<Item = Self::Color>,
        {
            self.0.fill_contiguous(area, colors)
        }

        fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
            self.0.fill_solid(area, color)
        }
    }

    pub struct OledDisplay {
        player_display: PlayerDisplay<DisplayWrapper>,
        blk_pin: Output<'static>,
        pub last_update: Instant,
    }

    impl OledDisplay {
        pub fn new(disp_res: DisplayResources) -> Self {
            let dc = Output::new(disp_res.pin7_spi_dc, Level::Low);
            let rst = Output::new(disp_res.pin20_spi_rst, Level::Low);

            let mut config = spi::Config::default();
            config.frequency = 40_000_000;
            let spi_disp = spi::Spi::new_txonly(
                disp_res.spi0,
                disp_res.pin18_spi_sck,
                disp_res.pin19_spi_tx,
                disp_res.dmach3,
                config,
            );
            let spi_dev = ExclusiveDevice::new(spi_disp, DummyCs, Delay);

            let di = SPIInterface::new(spi_dev, dc);

            let mut disp = Ili9488::new(
                di,
                rst,
                &mut Delay,
                Orientation::Landscape,
                Rgb666Mode,
            )
            .expect("could not init display");

            disp.clear_screen_fast(ili9488_rs::Rgb111::BLACK)
                .expect("could not clear display");
            disp.brightness(150).unwrap();
            
            let wrapper = DisplayWrapper(disp);
            let player = PlayerDisplay::new(wrapper);
            
            Self {
                player_display: player,
                blk_pin: Output::new(disp_res.pin22_blk_gnd, Level::Low),
                last_update: Instant::now(),
            }
        }

        pub fn turn_off_backlight(&mut self) {
            self.blk_pin.set_low();
        }
        pub fn turn_on_backlight(&mut self) {
            self.blk_pin.set_high();
            self.last_update = Instant::now();
        }

        // Delegates to PlayerDisplay
        pub fn set_fullscreen_vu_mode(&mut self, enable: bool) {
            self.player_display.set_fullscreen_vu_mode(enable);
        }

        pub fn draw_background(&mut self) {
            self.player_display.draw_background();
        }

        pub fn draw_layout_lines(&mut self) {
            self.player_display.draw_layout_lines();
        }

        pub fn draw_header_status(&mut self, input: &str, filter: &str) {
            self.player_display.draw_header_status(input, filter);
        }

        pub fn draw_volume(&mut self, vol_db: u8) {
            self.player_display.draw_volume(vol_db);
        }

        pub fn tick(&mut self) {
            self.player_display.tick();
        }

        pub fn draw_track_info(&mut self, title: &str, artist: &str, album: &str) {
            self.player_display.draw_track_info(title, artist, album);
        }

        pub fn redraw_track_info(&mut self) {
            self.player_display.redraw_track_info();
        }

        pub fn draw_vu_meter(&mut self, left: u8, right: u8, volume: u8) {
            self.player_display.draw_vu_meter(left, right, volume);
        }

        pub fn draw_progress_bar(&mut self, curr_time: &str, total_time: &str, progress: f32) {
            self.player_display.draw_progress_bar(curr_time, total_time, progress);
        }

        pub fn draw_footer(&mut self, format: &str, freq: &str, bit_depth: &str) {
            self.player_display.draw_footer(format, freq, bit_depth);
        }
        
        pub fn draw_powered_off(&mut self) {
            self.player_display.draw_powered_off();
        }

        pub fn clear_main_area(&mut self) {
            self.player_display.clear_main_area();
        }

        pub fn draw_fullscreen_vu_meter(&mut self, left: u8, right: u8, volume: u8) {
            self.player_display.draw_fullscreen_vu_meter(left, right, volume);
        }

        pub fn draw_fullscreen_vu_labels(&mut self) {
            self.player_display.draw_fullscreen_vu_labels();
        }

        pub fn draw_large_volume(&mut self, vol: u8) {
            self.player_display.draw_large_volume(vol);
        }
    }
}

#[cfg(target_arch = "arm")]
pub use hardware::*;
