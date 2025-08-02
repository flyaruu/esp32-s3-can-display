use core::{
    cmp::{max, min},
    convert::Infallible,
    error::Error,
    f32::consts::PI,
};

use alloc::{format, string::String};

use embedded_graphics::{
    Drawable,
    geometry::{Angle, Point},
    mono_font::{
        MonoTextStyle, MonoTextStyleBuilder,
        ascii::{FONT_8X13, FONT_10X20},
    },
    pixelcolor::{Rgb565, raw::RawU16},
    prelude::{Dimensions, DrawTarget, RgbColor},
    primitives::{
        Arc, Circle, Line, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, StyledDrawable,
    },
    text::Text,
};
use log::info;
use num_traits::Float;
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
    pub value: i32,
    pub indicated_value: i32,
    pub texts: [&'a str; 13],
    line1: String,
    line2: String,
    scaled_max: u64,
}

#[allow(dead_code)]
/// Static context for the dashboard, shouldn't change much after creation
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
    // const MAX_VALUE_SCALED: u64 = (MAX_VALUE * 360 / 300).to_u64().unwrap(); // Is this possible in const?
    pub fn new_speedo(texts: [&'a str; 13]) -> Self {
        let max_value_scaled: u64 = (MAX_VALUE * 360 / 300).to_u64().unwrap(); // scale max value to the (300 deg) range of the gauge
        Gauge {
            value: 0,
            indicated_value: 0,
            texts,
            line1: String::new(),
            line2: String::new(),
            scaled_max: max_value_scaled,
        }
    }

    pub fn set_line1(&mut self, value: String) {
        self.line1 = value;
    }

    pub fn get_line1(&'a mut self) -> &'a mut String {
        &mut self.line1
    }

    pub fn set_line2(&mut self, value: String) {
        self.line2 = value;
    }

    pub fn get_line2(&'a mut self) -> &'a mut String {
        &mut self.line2
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

    pub fn draw_static<D: DrawTarget<Color = Rgb565, Error = Infallible>>(
        &self,
        framebuffer: &mut D,
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

    pub fn draw_clear_mask<E: Error, D: DrawTarget<Color = Rgb565, Error = E>>(
        &self,
        framebuffer: &mut D,
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

    pub fn draw_dynamic<D: DrawTarget<Color = Rgb565, Error = Infallible>>(
        &self,
        framebuffer: &mut D,
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
                    text,
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
        // info!("Drawing dial at angle: {}", gauge_angle3);
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

        // TODO disable line so I can remove mut
        // write!(self.line1, "{}", self.value).unwrap();
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

impl<'a, const GAUGE_WIDTH: usize, const GAUGE_HEIGHT: usize>
    DashboardContext<'a, GAUGE_WIDTH, GAUGE_HEIGHT>
{
    pub fn new() -> Self {
        let r: f32 = (GAUGE_WIDTH as i32 / 2).to_f32().unwrap();
        let cx = (GAUGE_WIDTH / 2) as i32;
        let cy = (GAUGE_HEIGHT / 2) as i32;
        let centre = Point::new(cx, cy);
        let clearing_circle_bounds =
            Circle::with_center(centre, (2.0 * (r - L_OFFSET)).to_u32().unwrap()).bounding_box();
        let back_color = Rgb565::from(RawU16::from(0x0026));
        let gauge_color = Rgb565::from(RawU16::from(0x055D));
        let purple = Rgb565::from(RawU16::from(0xEA16));
        let needle_color = Rgb565::from(RawU16::from(0xF811));
        let outer_style = PrimitiveStyleBuilder::new()
            .stroke_color(gauge_color)
            .stroke_width(3)
            .build();
        let inner_style = PrimitiveStyleBuilder::new()
            .stroke_color(Rgb565::WHITE)
            .stroke_width(3)
            .build();
        let redline_style = PrimitiveStyleBuilder::new()
            .stroke_color(purple)
            .stroke_width(3)
            .build();
        let tick_style = PrimitiveStyleBuilder::new()
            .stroke_color(Rgb565::WHITE)
            .stroke_width(2)
            .build();
        let red_tick_style = PrimitiveStyleBuilder::new()
            .stroke_color(purple)
            .stroke_width(2)
            .build();
        let needle_style = PrimitiveStyleBuilder::new()
            .stroke_color(needle_color)
            .stroke_width(4)
            .build();
        let headlight_on_style = PrimitiveStyleBuilder::new()
            .fill_color(Rgb565::GREEN)
            .stroke_width(1)
            .stroke_color(Rgb565::GREEN)
            .build();
        let headlight_high_style = PrimitiveStyleBuilder::new()
            .fill_color(gauge_color)
            .stroke_width(1)
            .stroke_color(gauge_color)
            .build();

        let indicator_on_style = PrimitiveStyleBuilder::new()
            .stroke_color(Rgb565::GREEN)
            .stroke_width(2)
            .build();
        let blinker_on_style = PrimitiveStyleBuilder::new()
            .fill_color(Rgb565::GREEN)
            .build();
        let blinker_off_style = PrimitiveStyleBuilder::new()
            .fill_color(Rgb565::new(0x4, 0x8, 0x4))
            .build();
        // let color = Rgb565::new(0x33, 0x33, 0x33);

        let light_off_style = PrimitiveStyleBuilder::new()
            .stroke_color(Rgb565::new(0x4, 0x8, 0x4))
            .stroke_width(1)
            .fill_color(Rgb565::new(0x4, 0x8, 0x4))
            .build();
        let text_style = MonoTextStyleBuilder::new()
            .text_color(Rgb565::WHITE)
            .font(&FONT_8X13)
            .build();
        let red_text_style = MonoTextStyleBuilder::new()
            .text_color(purple)
            .font(&FONT_8X13)
            .build();

        let centre_text_style = MonoTextStyleBuilder::new()
            .text_color(Rgb565::WHITE)
            .font(&FONT_10X20)
            .build();

        let mut context: DashboardContext<GAUGE_WIDTH, GAUGE_HEIGHT> = DashboardContext {
            outer: [Point { x: 0, y: 0 }; 360],
            p_point: [Point { x: 0, y: 0 }; 360],
            l_point: [Point { x: 0, y: 0 }; 360],
            n_point: [Point { x: 0, y: 0 }; 360],
            centre,
            back_color,
            gauge_color,
            purple,
            needle_color,
            outer_style,
            inner_style,
            redline_style,
            tick_style,
            red_tick_style,
            needle_style,
            headlight_on_style,
            headlight_high_style,
            indicator_on_style,
            blinker_on_style,
            blinker_off_style,
            light_off_style,
            text_style,
            red_text_style,
            centre_text_style,
            clearing_circle_bounds,
        };
        for i in 0..360 {
            let a = ((i + 120) % 360) as i32;
            let angle_rad = a.to_f32().unwrap() * PI / 180.0;
            info!("i: {i} a: {a} a_rad: {angle_rad}");
            context.outer[i] = Point {
                x: ((r - OUTER_OFFSET) * angle_rad.cos()).to_i32().unwrap() + cx,
                y: ((r - OUTER_OFFSET) * angle_rad.sin()).to_i32().unwrap() + cy,
            };
            context.p_point[i] = Point {
                x: ((r - P_OFFSET) * angle_rad.cos()).to_i32().unwrap() + cx,
                y: ((r - P_OFFSET) * angle_rad.sin()).to_i32().unwrap() + cy,
            };
            context.l_point[i] = Point {
                x: ((r - L_OFFSET) * angle_rad.cos()).to_i32().unwrap() + cx,
                y: ((r - L_OFFSET) * angle_rad.sin()).to_i32().unwrap() + cy,
            };
            context.n_point[i] = Point {
                x: ((r - N_OFFSET) * angle_rad.cos()).to_i32().unwrap() + cx,
                y: ((r - N_OFFSET) * angle_rad.sin()).to_i32().unwrap() + cy,
            };
        }
        context
    }
}
