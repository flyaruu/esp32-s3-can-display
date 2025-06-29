use core::{
    cmp::{max, min},
    fmt::Write,
};

use alloc::format;

use embedded_graphics::{
    Drawable,
    framebuffer::Framebuffer,
    geometry::{Angle, Point, Size},
    mono_font::MonoTextStyle,
    pixelcolor::{
        Rgb565,
        raw::{BigEndian, RawU16},
    },
    primitives::{
        Arc, Circle, Line, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, StyledDrawable,
    },
    text::Text,
};
use heapless::String;
// use num_traits::ToPrimitive;
use num_traits::cast::ToPrimitive;

// use crate::dashboard::{DashboardContext, I_L_OFFSET, I_N_OFFSET, I_OUTER_OFFSET, I_P_OFFSET};
pub const OUTER_OFFSET: f32 = 10.0;
pub const P_OFFSET: f32 = 20.0;
pub const L_OFFSET: f32 = 40.0;
pub const N_OFFSET: f32 = 70.0;

pub const I_OUTER_OFFSET: u32 = 10;
pub const I_P_OFFSET: u32 = 20;
pub const I_L_OFFSET: u32 = 40;
pub const I_N_OFFSET: u32 = 70;

const MAX_CHANGE: i32 = 20;
pub struct Gauge<
    'a,
    const W: usize,
    const H: usize,
    const BUFFER: usize,
    const CLEAR_RADIUS: usize,
    const MAX_VALUE: usize,
> {
    pub bounding_box: Rectangle,
    pub value: i32,
    pub indicated_value: i32,
    pub texts: [&'a str; 13],
    line1: String<6>,
    line2: String<6>,
    scaled_max: u64,
}

#[allow(dead_code)]
pub struct DashboardContext<'a, const GAUGE_WIDTH: usize, const GAUGE_HEIGHT: usize> {
    pub outer: [Point; 360],
    pub p_point: [Point; 360],
    pub l_point: [Point; 360],
    pub n_point: [Point; 360],
    pub centre: Point,
    pub back_color: Rgb565,
    gauge_color: Rgb565,
    purple: Rgb565,
    needle_color: Rgb565,
    pub outer_style: PrimitiveStyle<Rgb565>,
    pub inner_style: PrimitiveStyle<Rgb565>,
    pub redline_style: PrimitiveStyle<Rgb565>,
    pub tick_style: PrimitiveStyle<Rgb565>,
    pub red_tick_style: PrimitiveStyle<Rgb565>,
    pub needle_style: PrimitiveStyle<Rgb565>,
    pub headlight_on_style: PrimitiveStyle<Rgb565>,
    pub indicator_on_style: PrimitiveStyle<Rgb565>,
    pub blinker_on_style: PrimitiveStyle<Rgb565>,
    pub blinker_off_style: PrimitiveStyle<Rgb565>,
    pub headlight_high_style: PrimitiveStyle<Rgb565>,
    pub light_off_style: PrimitiveStyle<Rgb565>,
    pub text_style: MonoTextStyle<'a, Rgb565>,
    pub red_text_style: MonoTextStyle<'a, Rgb565>,
    pub centre_text_style: MonoTextStyle<'a, Rgb565>,
    pub clearing_circle_bounds: Rectangle,
    //     let gauge_color = Rgb565::from(RawU16::from(0x055D));
    //     let purple = Rgb565::from(RawU16::from(0xEA16));
    //     let needle_color = Rgb565::from(RawU16::from(0xF811));
}

impl<
    'a,
    const W: usize,
    const H: usize,
    const BUFFER: usize,
    const CLEAR_RADIUS: usize,
    const MAX_VALUE: usize,
> Gauge<'a, W, H, BUFFER, CLEAR_RADIUS, MAX_VALUE>
{
    const CX: i32 = (W / 2) as i32;
    const CY: i32 = (H / 2) as i32;

    pub fn new_speedo(
        location: Point,
        texts: [&'a str; 13],
        line1: String<6>,
        line2: String<6>,
    ) -> Self {
        let size = Size::new(W as u32, H as u32);
        let max_value_scaled: u64 = (MAX_VALUE * 360 / 300).to_u64().unwrap(); // scale max value to the (300 deg) range of the gauge
        Gauge {
            bounding_box: Rectangle::new(location, size),
            value: 0,
            indicated_value: 0,
            texts,
            line1,
            line2,
            scaled_max: max_value_scaled,
        }
    }

    pub fn set_line1(&mut self, value: String<6>) {
        self.line1 = value;
    }

    pub fn set_line2(&mut self, value: String<6>) {
        self.line2 = value;
    }

    pub fn set_value(&mut self, value: i32) {
        self.value = value;
    }
    pub fn update_indicated(&mut self) {
        // info!("Before: Indicated: {} Value: {}",self.indicated_value,self.value);
        if self.indicated_value < self.value {
            self.indicated_value = min(self.indicated_value + MAX_CHANGE, self.value);
        }
        if self.indicated_value > self.value {
            self.indicated_value = max(self.indicated_value - MAX_CHANGE, self.value);
        }
    }

    pub fn draw_static(
        &self,
        framebuffer: &mut Framebuffer<Rgb565, RawU16, BigEndian, W, H, BUFFER>,
        context: &DashboardContext<W, H>,
    ) {
        Arc::with_center(
            Point {
                x: Self::CX,
                y: Self::CY,
            },
            W as u32 - I_OUTER_OFFSET,
            Angle::from_degrees(120.0),
            Angle::from_degrees(300.0),
        )
        .draw_styled(&context.outer_style, framebuffer)
        .unwrap();
        Arc::with_center(
            Point {
                x: Self::CX,
                y: Self::CY,
            },
            W as u32 - I_P_OFFSET,
            Angle::from_degrees(120.0),
            Angle::from_degrees(300.0),
        )
        .draw_styled(&context.inner_style, framebuffer)
        .unwrap();
        Arc::with_center(
            Point {
                x: Self::CX,
                y: Self::CY,
            },
            W as u32 - I_L_OFFSET,
            Angle::from_degrees(0.0),
            Angle::from_degrees(60.0),
        )
        .draw_styled(&context.redline_style, framebuffer)
        .unwrap();
        for i in 0..26 {
            let (tick, current_text_style) = if i < 20 {
                (context.tick_style, context.text_style)
            } else {
                (context.red_tick_style, context.red_text_style)
            };
            if i % 2 == 0 {
                Line::new(context.outer[i * 12], context.p_point[i * 12])
                    .draw_styled(&tick, framebuffer)
                    .unwrap();
                let text = format!("{}", i * 10);
                Text::with_alignment(
                    &text,
                    context.l_point[i * 12],
                    current_text_style,
                    embedded_graphics::text::Alignment::Center,
                )
                .draw(framebuffer)
                .unwrap();
            } else {
                Line::new(context.outer[i * 12], context.p_point[i * 12])
                    .draw_styled(&tick, framebuffer)
                    .unwrap();
            }
        }
    }

    pub fn draw_clear_mask(
        &self,
        framebuffer: &mut Framebuffer<Rgb565, RawU16, BigEndian, W, H, BUFFER>,
        context: &DashboardContext<W, H>,
    ) {
        Circle::with_center(
            Point {
                x: Self::CX,
                y: Self::CY,
            },
            CLEAR_RADIUS.to_u32().unwrap(),
        )
        .draw_styled(
            &PrimitiveStyleBuilder::new()
                .fill_color(context.back_color)
                .build(),
            framebuffer,
        )
        .unwrap();
    }

    pub fn draw_dynamic(
        &mut self,
        framebuffer: &mut Framebuffer<Rgb565, RawU16, BigEndian, W, H, BUFFER>,
        context: &DashboardContext<W, H>,
    ) {
        // Dynamic
        for i in 0..26 {
            let current_text_style = if i < 20 {
                context.text_style
            } else {
                context.red_text_style
            };
            if i % 2 == 0 {
                // TODO time this, could store these:
                let text: &str = self.texts[i >> 1];
                Text::with_alignment(
                    &text,
                    context.l_point[i * 12],
                    current_text_style,
                    embedded_graphics::text::Alignment::Center,
                )
                .draw(framebuffer)
                .unwrap();
            }
        }
        let gauge_angle3: usize = (self.indicated_value.to_f32().unwrap() * 360.0
            / self.scaled_max.to_f32().unwrap())
        .to_usize()
        .unwrap()
            % 360;
        // Big mistery: Uncommenting the following code will cause the screen to stop working. It starts, it prints to out, just no screen.
        // Even if the code is _never executed_
        // Compiler bug? Weird linker thing? I give up
        // if self.indicated_value > 10000 {
        //     let gauge_angle2: usize = (self.indicated_value * 360).try_into().unwrap();
        // }
        Line::new(context.l_point[gauge_angle3], context.n_point[gauge_angle3])
            .draw_styled(&context.needle_style, framebuffer)
            .unwrap();
        Arc::with_center(
            Point {
                x: Self::CX,
                y: Self::CY,
            },
            (W as u32 - I_N_OFFSET) / 2,
            Angle::from_degrees(100.0),
            Angle::from_degrees(340.0),
        )
        .draw_styled(&context.outer_style, framebuffer)
        .unwrap();

        write!(self.line1, "{}", self.value).unwrap();
        // self.set_line1(String::from(self.value));
        Text::with_alignment(
            &self.line1,
            context.centre,
            context.centre_text_style,
            embedded_graphics::text::Alignment::Center,
        )
        .draw(framebuffer)
        .unwrap();
        Text::with_alignment(
            &self.line2,
            Point::new(context.centre.x, context.centre.y + 18),
            context.centre_text_style,
            embedded_graphics::text::Alignment::Center,
        )
        .draw(framebuffer)
        .unwrap();
    }
}
