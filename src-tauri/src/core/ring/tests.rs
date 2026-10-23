use super::*;

const SIZES: [u32; 3] = [16, 22, 32];
const GLYPHS: [(&str, Glyph); 9] = [
    ("unknown", Glyph::Unknown),
    ("testing", Glyph::Testing(0.4)),
    (
        "good",
        Glyph::Result {
            health: Health::Good,
            stale: false,
        },
    ),
    (
        "fair",
        Glyph::Result {
            health: Health::Fair,
            stale: false,
        },
    ),
    (
        "poor",
        Glyph::Result {
            health: Health::Poor,
            stale: false,
        },
    ),
    (
        "stale",
        Glyph::Result {
            health: Health::Good,
            stale: true,
        },
    ),
    ("failed", Glyph::Failed),
    ("done", Glyph::Testing(1.0)),
    ("offline", Glyph::Offline),
];

struct Case {
    style: IconStyle,
    background: [u8; 3],
    tint: Option<[u8; 3]>,
    name: &'static str,
}

fn cases() -> [Case; 4] {
    [
        Case {
            style: IconStyle::Template,
            background: [0xf2, 0xf2, 0xf2],
            tint: Some([0, 0, 0]),
            name: "template-light",
        },
        Case {
            style: IconStyle::Template,
            background: [0x26, 0x26, 0x26],
            tint: Some([0xff, 0xff, 0xff]),
            name: "template-dark",
        },
        Case {
            style: IconStyle::LightTaskbar,
            background: [0xee, 0xee, 0xee],
            tint: None,
            name: "colour-light",
        },
        Case {
            style: IconStyle::DarkTaskbar,
            background: [0x20, 0x20, 0x20],
            tint: None,
            name: "colour-dark",
        },
    ]
}

fn luminance(rgb: [f64; 3]) -> f64 {
    let channel = |value: f64| {
        let value = value / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(rgb[0]) + 0.7152 * channel(rgb[1]) + 0.0722 * channel(rgb[2])
}

fn contrast(left: [f64; 3], right: [f64; 3]) -> f64 {
    let (a, b) = (luminance(left), luminance(right));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn composite(rgba: [u8; 4], tint: Option<[u8; 3]>, background: [u8; 3]) -> [f64; 3] {
    let rgb = tint.unwrap_or([rgba[0], rgba[1], rgba[2]]);
    let alpha = f64::from(rgba[3]) / 255.0;
    [0, 1, 2].map(|i| f64::from(rgb[i]) * alpha + f64::from(background[i]) * (1.0 - alpha))
}

fn min_contrast(case: &Case, size: u32, glyph: Glyph) -> f64 {
    let background = case.background.map(f64::from);
    raster(size, glyph, case.style)
        .iter()
        .filter(|pixel| pixel.coverage >= 1.0)
        .map(|pixel| {
            contrast(
                composite(pixel.rgba, case.tint, case.background),
                background,
            )
        })
        .fold(f64::INFINITY, f64::min)
}

fn alpha_at(size: u32, glyph: Glyph, x: f64, y: f64) -> u8 {
    let pixels = raster(size, glyph, IconStyle::Template);
    let (column, row) = ((x * f64::from(size)) as u32, (y * f64::from(size)) as u32);
    pixels[(row * size + column) as usize].rgba[3]
}

#[test]
fn every_glyph_reads_at_tray_sizes_on_light_and_dark_bars() {
    for case in cases() {
        for size in SIZES {
            for (name, glyph) in GLYPHS {
                let min = min_contrast(&case, size, glyph);
                assert!(
                    min.is_finite(),
                    "{} {size} {name} has no solid pixels",
                    case.name
                );
                assert!(min >= 3.0, "{} {size} {name} contrast {min:.2}", case.name);
            }
        }
    }
}

#[test]
fn template_varies_only_alpha() {
    for size in SIZES {
        for (_, glyph) in GLYPHS {
            let bytes = glyph_rgba(size, glyph, IconStyle::Template);
            assert_eq!(bytes.len(), (size * size * 4) as usize);
            assert!(bytes.chunks(4).all(|pixel| pixel[..3] == [0, 0, 0]));
        }
    }
}

#[test]
fn gauge_is_open_at_the_bottom_and_fills_clockwise() {
    let good = Glyph::Result {
        health: Health::Good,
        stale: false,
    };
    assert_eq!(alpha_at(32, good, 0.5, 0.95), 0, "bottom stays open");
    assert_eq!(alpha_at(32, good, 0.5, 0.06), 255, "top is filled");
    let third = Glyph::Testing(0.3);
    assert_eq!(alpha_at(32, third, 0.06, 0.5), 255, "left filled first");
    assert!(alpha_at(32, third, 0.94, 0.5) < 255, "right still track");
}

#[test]
fn health_shows_as_fill_and_poor_adds_a_centre_dot() {
    let glyph = |health| Glyph::Result {
        health,
        stale: false,
    };
    let filled = |glyph| {
        raster(32, glyph, IconStyle::Template)
            .iter()
            .filter(|pixel| pixel.rgba[3] == 255)
            .count()
    };
    assert!(filled(glyph(Health::Good)) > filled(glyph(Health::Fair)));
    assert!(filled(glyph(Health::Fair)) > filled(glyph(Health::Poor)) - 60);
    assert_eq!(alpha_at(32, glyph(Health::Good), 0.5, 0.5), 0);
    assert_eq!(alpha_at(32, glyph(Health::Poor), 0.5, 0.5), 255);
    assert_eq!(alpha_at(32, Glyph::Failed, 0.5, 0.5), 255);
    assert_ne!(
        glyph_rgba(32, Glyph::Unknown, IconStyle::Template),
        glyph_rgba(32, Glyph::Testing(0.0), IconStyle::Template)
    );
}

#[test]
fn offline_is_crossed_out() {
    assert_eq!(alpha_at(32, Glyph::Offline, 0.5, 0.5), 255);
    assert_eq!(alpha_at(32, Glyph::Offline, 0.38, 0.38), 0);
    assert_eq!(alpha_at(32, Glyph::Offline, 0.7, 0.3), 255);
    assert_ne!(
        glyph_rgba(32, Glyph::Offline, IconStyle::Template),
        glyph_rgba(32, Glyph::Failed, IconStyle::Template)
    );
}

#[test]
fn stale_result_is_dimmer_than_fresh() {
    let fresh = alpha_at(32, GLYPHS[2].1, 0.5, 0.06);
    let stale = alpha_at(32, GLYPHS[5].1, 0.5, 0.06);
    assert!(stale < fresh, "{stale} vs {fresh}");
}

#[test]
#[ignore = "writes docs/images/tray previews"]
fn write_previews() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/images/tray");
    std::fs::create_dir_all(&directory).unwrap();
    let cell = 40_u32;
    let width = cell * GLYPHS.len() as u32;
    let height = cell * (SIZES.len() * cases().len()) as u32;
    let mut sheet = vec![0_u8; (width * height * 3) as usize];
    for (band, case) in cases().iter().enumerate() {
        for (row, size) in SIZES.into_iter().enumerate() {
            for (column, (name, glyph)) in GLYPHS.into_iter().enumerate() {
                println!(
                    "{} {size}px {name}: min contrast {:.2}",
                    case.name,
                    min_contrast(case, size, glyph)
                );
                let pixels = raster(size, glyph, case.style);
                let top = (band * SIZES.len() + row) as u32 * cell;
                let left = column as u32 * cell;
                let inset = (cell - size) / 2;
                for y in 0..cell {
                    for x in 0..cell {
                        let mut rgb = case.background.map(f64::from);
                        if (inset..inset + size).contains(&x) && (inset..inset + size).contains(&y)
                        {
                            let pixel = &pixels[((y - inset) * size + x - inset) as usize];
                            rgb = composite(pixel.rgba, case.tint, case.background);
                        }
                        let offset = (((top + y) * width + left + x) * 3) as usize;
                        for (i, channel) in rgb.into_iter().enumerate() {
                            sheet[offset + i] = channel.round() as u8;
                        }
                    }
                }
            }
        }
    }
    let zoom = 4;
    let mut zoomed = vec![0_u8; (width * height * 3 * zoom * zoom) as usize];
    for y in 0..height * zoom {
        for x in 0..width * zoom {
            let source = (((y / zoom) * width + x / zoom) * 3) as usize;
            let target = ((y * width * zoom + x) * 3) as usize;
            zoomed[target..target + 3].copy_from_slice(&sheet[source..source + 3]);
        }
    }
    for (file, data, scale) in [
        ("contact-sheet.png", &sheet, 1),
        ("contact-sheet@4x.png", &zoomed, zoom),
    ] {
        let output = std::fs::File::create(directory.join(file)).unwrap();
        let mut encoder = png::Encoder::new(output, width * scale, height * scale);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(data)
            .unwrap();
    }
}
