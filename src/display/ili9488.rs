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

use crate::PlaybackMode;

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

// Shared buffer for line drawing to save RAM. Sized for the tallest region
// drawn in one blit: a BigInfo scrolling text strip (480×70) — scroll strips
// must blit atomically or the panel shows sheared text mid-update.
// Note: We use a static mutable buffer. Ensure single-threaded access (tick is sequential).
static mut LINE_BUFFER: [Rgb666; 480 * 70] = [Rgb666::new(0, 0, 0); 480 * 70];

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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    Normal = 0,
    VuMeter = 1,
    BigInfo = 2,
}

impl From<u8> for DisplayMode {
    fn from(val: u8) -> Self {
        match val {
            0 => DisplayMode::Normal,
            1 => DisplayMode::VuMeter,
            2 => DisplayMode::BigInfo,
            _ => DisplayMode::Normal,
        }
    }
}

pub struct PlayerDisplay<D> {
    pub display: D,
    track_artist: String<64>,
    track_title: String<64>,
    track_album: String<64>,
    scroll_tick: i32,
    scroll_accumulator: i32,
    last_total_time: String<16>,
    display_mode: DisplayMode,
    playback_mode: PlaybackMode,
    footer_format: String<16>,
    footer_freq: String<16>,
    footer_bit_depth: String<16>,
    force_redraw: bool,
    /// Last drawn side VU bar heights (px). `None` forces a full bar
    /// repaint; otherwise only the span between old and new level is drawn.
    last_vu_side: Option<(u32, u32)>,
    /// Last drawn fullscreen VU bar widths (px), same delta scheme.
    last_vu_full: Option<(u32, u32)>,
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
            scroll_accumulator: 0,
            last_total_time: String::new(),
            display_mode: DisplayMode::Normal,
            playback_mode: PlaybackMode::Sequential,
            footer_format: String::new(),
            footer_freq: String::new(),
            footer_bit_depth: String::new(),
            force_redraw: false,
            last_vu_side: None,
            last_vu_full: None,
        }
    }

    pub async fn tick(&mut self) {
        self.scroll_accumulator += 4; // Move 4 pixels per tick (at 50ms = 80px/sec)
        const SCROLL_THRESHOLD: i32 = 4;

        if self.scroll_accumulator >= SCROLL_THRESHOLD || self.force_redraw {
            self.scroll_tick += self.scroll_accumulator;
            self.scroll_accumulator = 0;

            let update_scrolling_only = !self.force_redraw;
            if self.display_mode == DisplayMode::Normal {
                self.draw_track_info_internal(update_scrolling_only).await;
            } else if self.display_mode == DisplayMode::BigInfo {
                self.draw_big_info_internal(update_scrolling_only).await;
            }
            self.force_redraw = false;
        }
    }

    /// Restarts the scroll animation from position zero.
    fn reset_scroll(&mut self) {
        self.scroll_tick = 0;
        self.scroll_accumulator = 0;
    }

    pub fn set_display_mode(&mut self, mode: DisplayMode) {
        self.display_mode = mode;
        self.invalidate_vu();
    }

    /// Forget the last drawn VU levels — call after painting over a meter
    /// area so the next VU frame repaints the bars fully.
    fn invalidate_vu(&mut self) {
        self.last_vu_side = None;
        self.last_vu_full = None;
    }

    pub fn draw_background(&mut self) {
        self.invalidate_vu();
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

        if self.display_mode != DisplayMode::BigInfo {
            embedded_graphics::primitives::Line::new(Point::new(0, 60), Point::new(479, 60))
                .into_styled(style_line)
                .draw(&mut self.display)
                .ok();
        }

        embedded_graphics::primitives::Line::new(Point::new(0, 265), Point::new(479, 265))
            .into_styled(style_line)
            .draw(&mut self.display)
            .ok();
    }

    pub fn draw_header_status(&mut self, input: &str, filter: &str) {
        if self.display_mode == DisplayMode::BigInfo {
            return;
        }
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

    pub fn draw_playback_mode(&mut self, mode: PlaybackMode) {
        self.playback_mode = mode;
        self.draw_playback_mode_section();
    }

    pub fn draw_footer(&mut self, format: &str, freq: &str, bit_depth: &str) {
        self.footer_format.clear();
        self.footer_format.push_str(format).ok();
        self.footer_freq.clear();
        self.footer_freq.push_str(freq).ok();
        self.footer_bit_depth.clear();
        self.footer_bit_depth.push_str(bit_depth).ok();

        self.draw_footer_internal();
    }

    pub fn redraw_footer(&mut self) {
        self.draw_footer_internal();
    }

    fn draw_playback_mode_section(&mut self) {
        let section_width = 480 / 4;
        let height = 49;
        let x = 0;
        let y = 271;

        let buffer_slice =
            unsafe { &mut LINE_BUFFER[..(section_width as usize * height as usize)] };
        buffer_slice.fill(COL_BG_BASE);

        let mut target = LineBuffer::new(buffer_slice, section_width as u32, height);

        // Use open_iconic_arrow for shuffle/repeat/etc

        // Based on reference image: B=arrow_right, Y=shuffle, W=loop_circular, X=loop_square

        let mode_icon = match self.playback_mode {
            PlaybackMode::Sequential => "B",

            PlaybackMode::Random => "Y",

            PlaybackMode::LoopSingle => "X",

            PlaybackMode::LoopQueue => "W",
        };

        if !mode_icon.is_empty() {
            let font_icon = FontRenderer::new::<fonts::u8g2_font_open_iconic_arrow_6x_t>();
            font_icon
                .render_aligned(
                    mode_icon,
                    Point::new(section_width / 2, height as i32 / 2),
                    VerticalPosition::Center,
                    HorizontalAlignment::Center,
                    FontColor::Transparent(COL_TEXT),
                    &mut target,
                )
                .ok();
        }

        self.display
            .fill_contiguous(
                &Rectangle::new(Point::new(x, y), Size::new(section_width as u32, height)),
                target.buffer.iter().cloned(),
            )
            .ok();
    }
    fn draw_footer_text_section(&mut self, section_idx: i32, text: &str) {
        let section_width = 480 / 4;
        let height = 49;
        let x = section_idx * section_width;
        let y = 271;

        let buffer_slice =
            unsafe { &mut LINE_BUFFER[..(section_width as usize * height as usize)] };
        buffer_slice.fill(COL_BG_BASE);

        let mut target = LineBuffer::new(buffer_slice, section_width as u32, height);

        let style_footer = U8g2TextStyle::new(fonts::u8g2_font_helvB24_tf, COL_TEXT);

        Text::with_text_style(
            text,
            Point::new(section_width / 2, height as i32 / 2),
            style_footer,
            TextStyleBuilder::new()
                .alignment(Alignment::Center)
                .baseline(Baseline::Middle)
                .build(),
        )
        .draw(&mut target)
        .ok();

        self.display
            .fill_contiguous(
                &Rectangle::new(Point::new(x, y), Size::new(section_width as u32, height)),
                target.buffer.iter().cloned(),
            )
            .ok();
    }

    fn draw_footer_internal(&mut self) {
        let f = self.footer_format.clone();
        let fr = self.footer_freq.clone();

        self.draw_playback_mode_section();
        self.draw_footer_text_section(1, &f);
        self.draw_footer_text_section(2, &fr);
    }

    pub fn draw_volume(&mut self, vol: u8) {
        if self.display_mode == DisplayMode::BigInfo {
            let width = 300;
            let height = 60;
            let screen_x = (480 - width as i32) / 2;
            let screen_y = 200;

            let buffer_slice = unsafe { &mut LINE_BUFFER[..(width * height) as usize] };
            buffer_slice.fill(COL_BG_BASE);

            let mut target = LineBuffer::new(buffer_slice, width, height);

            let style = U8g2TextStyle::new(fonts::u8g2_font_fub42_tf, COL_1);

            let mut vol_str = String::<16>::new();
            let db = (vol as f32 - 255.0) / 2.0;
            write!(&mut vol_str, "{:.1} dB", db).unwrap();

            Text::with_text_style(
                &vol_str,
                Point::new(width as i32 / 2, height as i32 / 2 + 10),
                style,
                TextStyleBuilder::new()
                    .alignment(Alignment::Center)
                    .baseline(Baseline::Middle)
                    .build(),
            )
            .draw(&mut target)
            .ok();

            self.display
                .fill_contiguous(
                    &Rectangle::new(Point::new(screen_x, screen_y), Size::new(width, height)),
                    target.buffer.iter().cloned(),
                )
                .ok();
            return;
        }

        let x = 340;
        let y = 25;
        let width = 130;
        let height = 30;

        let buffer_slice = unsafe { &mut LINE_BUFFER[..(width * height) as usize] };
        buffer_slice.fill(COL_BG_BASE);

        let mut target = LineBuffer::new(buffer_slice, width, height);
        let style_value = U8g2TextStyle::new(fonts::u8g2_font_helvB18_tf, COL_1);

        let text_style_right = TextStyleBuilder::new()
            .alignment(Alignment::Right)
            .baseline(Baseline::Alphabetic)
            .build();

        let mut vol_str = String::<32>::new();
        let db = (vol as f32 - 255.0) / 2.0;
        write!(&mut vol_str, "{:.1} dB", db).unwrap();

        Text::with_text_style(
            &vol_str,
            Point::new(width as i32 - 5, 20),
            style_value,
            text_style_right,
        )
        .draw(&mut target)
        .ok();

        self.display
            .fill_contiguous(
                &Rectangle::new(Point::new(x, y), Size::new(width, height)),
                target.buffer.iter().cloned(),
            )
            .ok();
    }


    pub fn draw_track_info(&mut self, title: &str, artist: &str, album: &str) {
        self.track_title.clear();
        self.track_title.push_str(title).ok();
        self.track_artist.clear();
        self.track_artist.push_str(artist).ok();
        self.track_album.clear();
        self.track_album.push_str(album).ok();
        self.reset_scroll();
        self.force_redraw = true;
    }

    pub fn redraw_track_info(&mut self) {
        self.reset_scroll();
        self.force_redraw = true;
    }

    pub fn clear_track_info(&mut self) {
        self.track_title.clear();
        self.track_artist.clear();
        self.track_album.clear();
        self.reset_scroll();
        self.force_redraw = true;
    }

    async fn draw_scrolling_text_line(
        &mut self,
        style: U8g2TextStyle<Rgb666>,
        text: &str,
        y: i32,
        height: u32,
        content_width: u32,
        center_x: i32,
        update_scrolling_only: bool,
    ) {
        let width = style
            .measure_string(text, Point::zero(), Baseline::Middle)
            .bounding_box
            .size
            .width as i32;

        if update_scrolling_only && width <= content_width as i32 {
            return;
        }

        // Render the whole strip and blit it in ONE transfer: chunked blits
        // (the previous 8-row scheme) let the panel display a mix of old and
        // new scroll offsets mid-update — visible as sheared, ghosting text.
        let buffer_slice =
            unsafe { &mut LINE_BUFFER[..(content_width as usize * height as usize)] };
        buffer_slice.fill(COL_BG_BASE);

        let mut target = LineBuffer::new(buffer_slice, content_width, height);
        let local_y = height as i32 / 2;

        if width <= content_width as i32 {
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

            if x + width < content_width as i32 {
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

        self.display
            .fill_contiguous(
                &Rectangle::new(
                    Point::new(
                        if self.display_mode == DisplayMode::Normal {
                            VU_MARGIN_X
                        } else {
                            0
                        },
                        y - (height as i32 / 2),
                    ),
                    Size::new(content_width, height),
                ),
                target.buffer.iter().cloned(),
            )
            .ok();

        // One yield per line keeps USB and command handling responsive
        // between the ~10-20ms blits.
        #[cfg(target_arch = "arm")]
        embassy_futures::yield_now().await;
    }

    async fn draw_big_info_internal(&mut self, update_scrolling_only: bool) {
        let style_title = U8g2TextStyle::new(fonts::u8g2_font_fub42_tf, Rgb666::WHITE);
        let style_artist = U8g2TextStyle::new(fonts::u8g2_font_fub42_tf, COL_1);

        let content_width = 480;
        let center_x = content_width / 2;

        self.draw_scrolling_text_line(
            style_artist,
            &self.track_artist.clone(),
            55,
            70,
            content_width as u32,
            center_x,
            update_scrolling_only,
        )
        .await;
        self.draw_scrolling_text_line(
            style_title,
            &self.track_title.clone(),
            135,
            70,
            content_width as u32,
            center_x,
            update_scrolling_only,
        )
        .await;
    }

    async fn draw_track_info_internal(&mut self, update_scrolling_only: bool) {
        let style_song = U8g2TextStyle::new(fonts::u8g2_font_helvB24_tf, Rgb666::WHITE);
        let style_artist = U8g2TextStyle::new(fonts::u8g2_font_helvB18_tf, COL_1);
        let style_album = U8g2TextStyle::new(fonts::u8g2_font_helvB18_tf, COL_TEXT);

        let content_width = 480 - (VU_MARGIN_X * 2);
        let center_x = content_width / 2;

        self.draw_scrolling_text_line(
            style_song,
            &self.track_title.clone(),
            90,
            50,
            content_width as u32,
            center_x,
            update_scrolling_only,
        )
        .await;
        self.draw_scrolling_text_line(
            style_artist,
            &self.track_artist.clone(),
            135,
            35,
            content_width as u32,
            center_x,
            update_scrolling_only,
        )
        .await;
        self.draw_scrolling_text_line(
            style_album,
            &self.track_album.clone(),
            180,
            45,
            content_width as u32,
            center_x,
            update_scrolling_only,
        )
        .await;
    }

    pub fn draw_vu_meter(&mut self, left: u8, right: u8, _volume: u8) {
        if self.display_mode == DisplayMode::BigInfo {
            return;
        }
        let h_left = (f32::from(left) / 255.0 * VU_MAX_HEIGHT as f32) as u32;
        let h_right = (f32::from(right) / 255.0 * VU_MAX_HEIGHT as f32) as u32;

        // Delta drawing: only the span between the previous and current
        // level is repainted — a full 2×10×202 px repaint at the 20 Hz VU
        // rate would keep the SPI bus needlessly busy.
        let prev = self.last_vu_side;
        self.last_vu_side = Some((h_left, h_right));
        match prev {
            None => {
                self.draw_vert_vu_span(0, 0, VU_MAX_HEIGHT, h_left);
                self.draw_vert_vu_span(480 - VU_WIDTH as i32, 0, VU_MAX_HEIGHT, h_right);
            }
            Some((old_l, old_r)) => {
                self.draw_vert_vu_span(0, old_l.min(h_left), old_l.max(h_left), h_left);
                self.draw_vert_vu_span(480 - VU_WIDTH as i32, old_r.min(h_right), old_r.max(h_right), h_right);
            }
        }
    }

    /// Repaints rows `from..to` (px above the bar's bottom) of one vertical
    /// VU bar at `x`, filled up to `level`: zone colors below the level,
    /// background above it.
    fn draw_vert_vu_span(&mut self, x: i32, from: u32, to: u32, level: u32) {
        let green_limit = (VU_MAX_HEIGHT as f32 * 0.7) as u32;
        let orange_limit = (VU_MAX_HEIGHT as f32 * 0.9) as u32;
        let bottom_y = VU_TOP_Y + VU_MAX_HEIGHT as i32;
        let zones = [
            (0, green_limit.min(level), COL_1),
            (green_limit.min(level), orange_limit.min(level), COL_TEXT),
            (orange_limit.min(level), level, COL_VU_MAX),
            (level, VU_MAX_HEIGHT, COL_BG_BASE),
        ];
        for (z_start, z_end, color) in zones {
            let s = from.max(z_start);
            let e = to.min(z_end);
            if e > s {
                Rectangle::new(Point::new(x, bottom_y - e as i32), Size::new(VU_WIDTH, e - s))
                    .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(color))
                    .draw(&mut self.display)
                    .ok();
            }
        }
    }

    pub fn draw_bar(&mut self, progress: f32) {
        let buffer_slice = unsafe { &mut LINE_BUFFER[..(BAR_WIDTH * BAR_HEIGHT) as usize] };
        buffer_slice.fill(COL_BG_BASE); // Default background

        let mut target = LineBuffer::new(buffer_slice, BAR_WIDTH, BAR_HEIGHT);

        let style_progress_fill = embedded_graphics::primitives::PrimitiveStyle::with_fill(COL_1);
        let style_progress_bg =
            embedded_graphics::primitives::PrimitiveStyle::with_fill(COL_BG_BASE);
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
        self.display
            .fill_contiguous(
                &Rectangle::new(Point::new(BAR_X, BAR_Y), Size::new(BAR_WIDTH, BAR_HEIGHT)),
                target.buffer.iter().cloned(),
            )
            .ok();
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

        self.display
            .fill_contiguous(
                &Rectangle::new(Point::new(x, y), Size::new(width, height)),
                target.buffer.iter().cloned(),
            )
            .ok();
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

        self.display
            .fill_contiguous(
                &Rectangle::new(Point::new(x, y), Size::new(width, height)),
                target.buffer.iter().cloned(),
            )
            .ok();
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

    pub fn draw_powered_off(&mut self) {
        self.invalidate_vu();
        let font_huge = FontRenderer::new::<fonts::u8g2_font_fub42_tf>();

        Rectangle::new(Point::new(0, 0), self.display.size())
            .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(
                COL_BG_BASE,
            ))
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
        self.invalidate_vu();
        let y_start = if self.display_mode == DisplayMode::BigInfo {
            0
        } else {
            62
        };
        let height = 265 - y_start;

        Rectangle::new(Point::new(0, y_start), Size::new(480, height as u32))
            .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(
                COL_BG_BASE,
            ))
            .draw(&mut self.display)
            .ok();
    }

    pub fn draw_fullscreen_vu_meter(&mut self, left: u8, right: u8, _volume: u8) {
        const MAX_WIDTH: u32 = 400;
        const L_Y: i32 = 100;
        const R_Y: i32 = 180;

        let w_left = (f32::from(left) / 255.0 * MAX_WIDTH as f32) as u32;
        let w_right = (f32::from(right) / 255.0 * MAX_WIDTH as f32) as u32;

        // Delta drawing: a full repaint is 2×400×40 px per frame — ~19 ms of
        // SPI time, ~40% duty at the 20 Hz VU rate. Only the span between
        // the previous and current level actually changes.
        let prev = self.last_vu_full;
        self.last_vu_full = Some((w_left, w_right));
        match prev {
            None => {
                self.draw_horiz_vu_span(L_Y, 0, MAX_WIDTH, w_left);
                self.draw_horiz_vu_span(R_Y, 0, MAX_WIDTH, w_right);
            }
            Some((old_l, old_r)) => {
                self.draw_horiz_vu_span(L_Y, old_l.min(w_left), old_l.max(w_left), w_left);
                self.draw_horiz_vu_span(R_Y, old_r.min(w_right), old_r.max(w_right), w_right);
            }
        }
    }

    /// Repaints columns `from..to` (px from the bar's left edge) of one
    /// fullscreen VU bar at `y`, filled up to `level`: zone colors below the
    /// level, dim background beyond it.
    fn draw_horiz_vu_span(&mut self, y: i32, from: u32, to: u32, level: u32) {
        const MAX_WIDTH: u32 = 400;
        const BAR_HEIGHT: u32 = 40;
        const START_X: i32 = (480 - MAX_WIDTH as i32) / 2;
        let green_limit = (MAX_WIDTH as f32 * 0.7) as u32;
        let orange_limit = (MAX_WIDTH as f32 * 0.9) as u32;
        let zones = [
            (0, green_limit.min(level), COL_1),
            (green_limit.min(level), orange_limit.min(level), COL_TEXT),
            (orange_limit.min(level), level, COL_VU_MAX),
            (level, MAX_WIDTH, Rgb666::CSS_DIM_GRAY),
        ];
        for (z_start, z_end, color) in zones {
            let s = from.max(z_start);
            let e = to.min(z_end);
            if e > s {
                Rectangle::new(Point::new(START_X + s as i32, y), Size::new(e - s, BAR_HEIGHT))
                    .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(color))
                    .draw(&mut self.display)
                    .ok();
            }
        }
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
        let width = 350;
        let height = 60;

        let buffer_slice = unsafe { &mut LINE_BUFFER[..(width * height) as usize] };
        buffer_slice.fill(COL_BG_BASE);

        let mut target = LineBuffer::new(buffer_slice, width as u32, height as u32);

        let style = U8g2TextStyle::new(fonts::u8g2_font_fub42_tf, COL_1);

        let mut vol_str = String::<16>::new();
        let db = (vol as f32 - 255.0) / 2.0;
        write!(&mut vol_str, "{:.1} dB", db).unwrap();

        Text::with_text_style(
            &vol_str,
            Point::new(width / 2, height / 2 + 10),
            style,
            TextStyleBuilder::new()
                .alignment(Alignment::Center)
                .baseline(Baseline::Middle)
                .build(),
        )
        .draw(&mut target)
        .ok();

        let screen_x = (480 - width) / 2;
        let screen_y = 62 + (203 - height) / 2;

        self.display
            .fill_contiguous(
                &Rectangle::new(
                    Point::new(screen_x, screen_y),
                    Size::new(width as u32, height as u32),
                ),
                target.buffer.iter().cloned(),
            )
            .ok();
    }
}

// Hardware Specifics (Targeted for embedded)
#[cfg(target_arch = "arm")]
mod hardware {
    use super::*;
    use crate::DisplayResources;
    use defmt_rtt as _;
    use mipidsi::interface::SpiInterface;
    use embassy_futures::yield_now;
    use embassy_rp::gpio::{Level, Output};
    use embassy_rp::spi;
    use embassy_time::{Delay, Instant};
    use embedded_hal_bus::spi::ExclusiveDevice;

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

    type SpiDeviceDisp = ExclusiveDevice<
        spi::Spi<'static, embassy_rp::peripherals::SPI0, spi::Async>,
        DummyCs,
        Delay,
    >;

    type Display =
        mipidsi::Display<SpiInterface<'static, SpiDeviceDisp, Output<'static>>, mipidsi::models::ILI9488Rgb666, Output<'static>>;

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

    /// SPI pixel-staging buffer for the mipidsi interface — pixels are
    /// converted into it and flushed chunk by chunk, so a bigger buffer
    /// means fewer blocking writes per blit. `OledDisplay` is constructed
    /// once, so the cell's single `init` never panics.
    static MII_BUFFER: static_cell::StaticCell<[u8; 4096]> = static_cell::StaticCell::new();

    impl OledDisplay {
        pub fn new(disp_res: DisplayResources) -> Self {
            let dc = Output::new(disp_res.pin7_spi_dc, Level::Low);
            let rst = Output::new(disp_res.pin20_spi_rst, Level::Low);

            let mut config = spi::Config::default();
            // 2× over the ILI9488's 20 MHz spec already; 62.5 MHz (RP2040
            // max) was tried 2026-07 and the panel failed to even
            // initialize (white screen) — do not raise this again.
            config.frequency = 40_000_000;
            let spi_disp = spi::Spi::new_txonly(
                disp_res.spi0,
                disp_res.pin18_spi_sck,
                disp_res.pin19_spi_tx,
                disp_res.dmach3,
                config,
            );
            let spi_dev = ExclusiveDevice::new(spi_disp, DummyCs, Delay);

            let buffer = MII_BUFFER.init([0; 4096]);
            let di = SpiInterface::new(spi_dev, dc, buffer);

            let mut disp = mipidsi::Builder::new(mipidsi::models::ILI9488Rgb666, di)
                .reset_pin(rst)
                .init(&mut Delay)
                .expect("could not init display");
                
            disp.set_orientation(mipidsi::options::Orientation::new().rotate(mipidsi::options::Rotation::Deg270).flip_horizontal()).unwrap();

            disp.clear(Rgb666::BLACK)
                .expect("could not clear display");

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

        // Delegates to PlayerDisplay

        pub fn set_display_mode(&mut self, mode: DisplayMode) {
            self.player_display.set_display_mode(mode);
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

        pub fn draw_playback_mode(&mut self, mode: PlaybackMode) {
            self.player_display.draw_playback_mode(mode);
        }

        pub fn draw_volume(&mut self, vol_db: u8) {
            self.player_display.draw_volume(vol_db);
        }

        pub async fn tick(&mut self) {
            self.player_display.tick().await;
        }

        pub fn draw_track_info(&mut self, title: &str, artist: &str, album: &str) {
            self.player_display.draw_track_info(title, artist, album);
        }

        pub fn redraw_track_info(&mut self) {
            self.player_display.redraw_track_info();
        }

        pub fn clear_track_info(&mut self) {
            self.player_display.clear_track_info();
        }

    pub fn draw_vu_meter(&mut self, left: u8, right: u8, volume: u8) {
            self.player_display.draw_vu_meter(left, right, volume);
        }

        pub fn draw_progress_bar(&mut self, curr_time: &str, total_time: &str, progress: f32) {
            self.player_display
                .draw_progress_bar(curr_time, total_time, progress);
        }

        pub fn draw_footer(&mut self, format: &str, freq: &str, bit_depth: &str) {
            self.player_display.draw_footer(format, freq, bit_depth);
        }

        pub fn redraw_footer(&mut self) {
            self.player_display.redraw_footer();
        }

        pub fn draw_powered_off(&mut self) {
            self.player_display.draw_powered_off();
        }

        pub fn clear_main_area(&mut self) {
            self.player_display.clear_main_area();
        }

    pub fn draw_fullscreen_vu_meter(&mut self, left: u8, right: u8, volume: u8) {
            self.player_display
                .draw_fullscreen_vu_meter(left, right, volume);
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
