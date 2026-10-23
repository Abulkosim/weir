//! Tray glyph: a 270° gauge open at the bottom, like the app icon.

use super::meaning::Health;
use std::f64::consts::{FRAC_PI_2, TAU};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconStyle {
    Template,
    LightTaskbar,
    DarkTaskbar,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Glyph {
    /// Never tested: the empty track only.
    Unknown,
    /// Test running, 0–1.
    Testing(f64),
    /// Last result; `stale` dims it.
    Result { health: Health, stale: bool },
    /// Last test failed: empty gauge with a centre dot.
    Failed,
}

#[derive(Clone, Copy, PartialEq)]
enum Part {
    Arc,
    Track,
    Mark,
}

struct Palette {
    good: [u8; 3],
    fair: [u8; 3],
    poor: [u8; 3],
    progress: [u8; 3],
    track: [u8; 3],
    neutral: [u8; 3],
}

const LIGHT: Palette = Palette {
    good: [0x06, 0x6f, 0x69],
    fair: [0x9a, 0x5b, 0x0b],
    poor: [0xb4, 0x23, 0x18],
    progress: [0x06, 0x6f, 0x69],
    track: [0x7a, 0x7a, 0x7a],
    neutral: [0x4a, 0x4a, 0x4a],
};

const DARK: Palette = Palette {
    good: [0x3f, 0xd6, 0xc8],
    fair: [0xf0, 0xb4, 0x5e],
    poor: [0xff, 0x8f, 0x84],
    progress: [0x3f, 0xd6, 0xc8],
    track: [0x7a, 0x7a, 0x7a],
    neutral: [0xc8, 0xc8, 0xc8],
};

const SAMPLES: usize = 8;
const OUTER: f64 = 0.47;
const STROKE: f64 = 0.17;
const TRACK: f64 = 0.55;
const DOT: f64 = 0.15;
const START: f64 = 0.625;
const SPAN: f64 = 0.75;

fn fill(glyph: Glyph) -> Option<f64> {
    match glyph {
        Glyph::Unknown => None,
        Glyph::Testing(progress) => Some(progress.clamp(0.02, 1.0)),
        Glyph::Result { health, .. } => Some(match health {
            Health::Good => 1.0,
            Health::Fair => 2.0 / 3.0,
            Health::Poor => 1.0 / 3.0,
        }),
        Glyph::Failed => Some(0.0),
    }
}

fn part_at(dx: f64, dy: f64, size: f64, glyph: Glyph) -> Option<Part> {
    let stroke = STROKE * size;
    let mid = OUTER * size - stroke / 2.0;
    let radius = dx.hypot(dy);
    let mark = matches!(
        glyph,
        Glyph::Failed
            | Glyph::Result {
                health: Health::Poor,
                ..
            }
    );
    if mark && radius <= DOT * size {
        return Some(Part::Mark);
    }
    let turn = (dy.atan2(dx) + FRAC_PI_2).rem_euclid(TAU) / TAU;
    let along = (turn - START).rem_euclid(1.0);
    let on_gauge = along <= SPAN;
    let in_band = (radius - mid).abs() <= stroke / 2.0;
    let cap = |position: f64| {
        let theta = (START + position * SPAN) * TAU - FRAC_PI_2;
        (dx - mid * theta.cos()).hypot(dy - mid * theta.sin()) <= stroke / 2.0
    };
    let value = fill(glyph).unwrap_or(0.0);
    if value > 0.0 && ((in_band && on_gauge && along <= value * SPAN) || cap(0.0) || cap(value)) {
        return Some(Part::Arc);
    }
    let end = (START + SPAN) * TAU - FRAC_PI_2;
    let track_cap = (dx - mid * end.cos()).hypot(dy - mid * end.sin()) <= stroke * TRACK / 2.0;
    let track_band = (radius - mid).abs() <= stroke * TRACK / 2.0;
    (track_band && on_gauge || track_cap).then_some(Part::Track)
}

fn color(part: Part, glyph: Glyph, style: IconStyle) -> ([u8; 3], f64) {
    let stale = matches!(glyph, Glyph::Result { stale: true, .. });
    let palette = match style {
        IconStyle::Template => {
            let alpha = match part {
                Part::Arc | Part::Mark if stale => 0.6,
                Part::Arc | Part::Mark => 1.0,
                Part::Track => 0.5,
            };
            return ([0, 0, 0], alpha);
        }
        IconStyle::LightTaskbar => &LIGHT,
        IconStyle::DarkTaskbar => &DARK,
    };
    let rgb = match (part, glyph) {
        (Part::Track, _) => palette.track,
        (_, Glyph::Unknown | Glyph::Failed) => palette.neutral,
        (_, Glyph::Testing(_)) => palette.progress,
        (_, Glyph::Result { stale: true, .. }) => palette.neutral,
        (_, Glyph::Result { health, .. }) => match health {
            Health::Good => palette.good,
            Health::Fair => palette.fair,
            Health::Poor => palette.poor,
        },
    };
    (rgb, 1.0)
}

pub struct Pixel {
    pub coverage: f64,
    pub rgba: [u8; 4],
}

pub fn raster(size: u32, glyph: Glyph, style: IconStyle) -> Vec<Pixel> {
    let extent = f64::from(size);
    let centre = extent / 2.0;
    let total = (SAMPLES * SAMPLES) as f64;
    let mut pixels = Vec::with_capacity((size * size) as usize);
    for y in 0..size {
        for x in 0..size {
            let (mut covered, mut alpha, mut channels) = (0.0, 0.0, [0.0_f64; 3]);
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let dx = f64::from(x) + (sx as f64 + 0.5) / SAMPLES as f64 - centre;
                    let dy = f64::from(y) + (sy as f64 + 0.5) / SAMPLES as f64 - centre;
                    if let Some(part) = part_at(dx, dy, extent, glyph) {
                        let (rgb, weight) = color(part, glyph, style);
                        covered += 1.0;
                        alpha += weight;
                        for (channel, component) in channels.iter_mut().zip(rgb) {
                            *channel += f64::from(component) * weight;
                        }
                    }
                }
            }
            let rgb = channels.map(|channel| {
                if alpha > 0.0 {
                    (channel / alpha).round() as u8
                } else {
                    0
                }
            });
            pixels.push(Pixel {
                coverage: covered / total,
                rgba: [
                    rgb[0],
                    rgb[1],
                    rgb[2],
                    (alpha / total * 255.0).round() as u8,
                ],
            });
        }
    }
    pixels
}

pub fn glyph_rgba(size: u32, glyph: Glyph, style: IconStyle) -> Vec<u8> {
    raster(size, glyph, style)
        .into_iter()
        .flat_map(|pixel| pixel.rgba)
        .collect()
}

#[cfg(test)]
mod tests;
