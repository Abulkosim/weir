//! A small drawing layer over pdf-writer: A4 pages, top-left coordinates, and the
//! two built-in Helvetica faces with their metrics, so nothing is embedded.

use pdf_writer::{Content, Name, Pdf, Rect, Ref, Str, TextStr};

pub const WIDTH: f32 = 595.28;
pub const HEIGHT: f32 = 841.89;
pub const MARGIN: f32 = 48.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Font {
    Regular,
    Bold,
}

impl Font {
    fn name(self) -> Name<'static> {
        match self {
            Self::Regular => Name(b"F1"),
            Self::Bold => Name(b"F2"),
        }
    }
}

pub type Rgb = [f32; 3];

const fn hex(value: u32) -> Rgb {
    [
        ((value >> 16) & 0xff) as f32 / 255.0,
        ((value >> 8) & 0xff) as f32 / 255.0,
        (value & 0xff) as f32 / 255.0,
    ]
}

pub const INK: Rgb = hex(0x0e1b22);
pub const SECONDARY: Rgb = hex(0x4b5d66);
pub const TERTIARY: Rgb = hex(0x82939b);
pub const LINE: Rgb = hex(0xe2eaed);
pub const SOFT: Rgb = hex(0xf1f5f6);
pub const ACCENT: Rgb = hex(0x0a8f88);
pub const WARNING: Rgb = hex(0xa8650f);
pub const DANGER: Rgb = hex(0xc0392b);

/// Helvetica and Helvetica-Bold advance widths for ASCII 32–126, per 1000 units (Adobe AFM).
const REGULAR: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667,
    611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500,
    222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];
const BOLD: [u16; 95] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611, 975, 722, 722, 722, 722, 667,
    611, 778, 722, 278, 556, 722, 611, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 333, 278, 333, 584, 556, 333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556,
    278, 889, 611, 611, 611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584,
];

/// WinAnsi bytes for `text`. Characters outside it become "?".
pub fn encode(text: &str) -> Vec<u8> {
    text.chars()
        .map(|char| match char {
            ' '..='~' => char as u8,
            '’' => 0x92,
            '‘' => 0x91,
            '–' => 0x96,
            '—' => 0x97,
            '•' => 0x95,
            '…' => 0x85,
            '·' => 0xb7,
            '×' => 0xd7,
            '\u{a0}' => b' ',
            _ => b'?',
        })
        .collect()
}

fn advance(byte: u8, font: Font) -> u16 {
    let table = match font {
        Font::Regular => &REGULAR,
        Font::Bold => &BOLD,
    };
    match byte {
        32..=126 => table[usize::from(byte - 32)],
        0x96 => 556,
        0x95 => 350,
        0x97 | 0x85 => 1000,
        0xb7 => 278,
        0xd7 => 584,
        0x91 | 0x92 => match font {
            Font::Regular => 222,
            Font::Bold => 278,
        },
        _ => 556,
    }
}

pub fn width(text: &str, font: Font, size: f32) -> f32 {
    encode(text)
        .into_iter()
        .map(|byte| f32::from(advance(byte, font)))
        .sum::<f32>()
        * size
        / 1000.0
}

/// Cuts `text` to fit `max` points, ending with "…".
pub fn fit(text: &str, font: Font, size: f32, max: f32) -> String {
    if width(text, font, size) <= max {
        return text.to_owned();
    }
    let mut cut: String = text.to_owned();
    while !cut.is_empty() && width(&format!("{cut}…"), font, size) > max {
        cut.pop();
    }
    format!("{}…", cut.trim_end())
}

/// Splits `text` into lines no wider than `max` points.
pub fn wrap(text: &str, font: Font, size: f32, max: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if width(&candidate, font, size) > max && !line.is_empty() {
            lines.push(std::mem::replace(&mut line, word.to_owned()));
        } else {
            line = candidate;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

pub struct Page {
    content: Content,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            content: Content::new(),
        }
    }
}

impl Page {
    pub fn new() -> Self {
        Self::default()
    }

    /// Text with its baseline `y` points below the top of the page.
    pub fn text(&mut self, x: f32, y: f32, text: &str, font: Font, size: f32, color: Rgb) {
        let bytes = encode(text);
        self.content
            .set_fill_rgb(color[0], color[1], color[2])
            .begin_text()
            .set_font(font.name(), size)
            .next_line(x, HEIGHT - y)
            .show(Str(&bytes))
            .end_text();
    }

    pub fn text_right(
        &mut self,
        right: f32,
        y: f32,
        text: &str,
        font: Font,
        size: f32,
        color: Rgb,
    ) {
        self.text(right - width(text, font, size), y, text, font, size, color);
    }

    pub fn rect(&mut self, x: f32, y: f32, width: f32, height: f32, color: Rgb) {
        self.content
            .set_fill_rgb(color[0], color[1], color[2])
            .rect(x, HEIGHT - y - height, width, height)
            .fill_nonzero();
    }

    pub fn line(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
        stroke: f32,
        color: Rgb,
        dashed: bool,
    ) {
        self.content.save_state();
        if dashed {
            self.content.set_dash_pattern([3.0, 3.0], 0.0);
        }
        self.content
            .set_line_width(stroke)
            .set_stroke_rgb(color[0], color[1], color[2])
            .move_to(from.0, HEIGHT - from.1)
            .line_to(to.0, HEIGHT - to.1)
            .stroke()
            .restore_state();
    }

    pub fn polyline(&mut self, points: &[(f32, f32)], stroke: f32, color: Rgb) {
        let Some((first, rest)) = points.split_first() else {
            return;
        };
        self.content
            .save_state()
            .set_line_width(stroke)
            .set_line_join(pdf_writer::types::LineJoinStyle::RoundJoin)
            .set_line_cap(pdf_writer::types::LineCapStyle::RoundCap)
            .set_stroke_rgb(color[0], color[1], color[2])
            .move_to(first.0, HEIGHT - first.1);
        for point in rest {
            self.content.line_to(point.0, HEIGHT - point.1);
        }
        self.content.stroke().restore_state();
    }

    pub fn dot(&mut self, center: (f32, f32), radius: f32, color: Rgb) {
        const K: f32 = 0.552_284_8;
        let (x, y) = (center.0, HEIGHT - center.1);
        let (r, k) = (radius, radius * K);
        self.content
            .set_fill_rgb(color[0], color[1], color[2])
            .move_to(x + r, y)
            .cubic_to(x + r, y + k, x + k, y + r, x, y + r)
            .cubic_to(x - k, y + r, x - r, y + k, x - r, y)
            .cubic_to(x - r, y - k, x - k, y - r, x, y - r)
            .cubic_to(x + k, y - r, x + r, y - k, x + r, y)
            .fill_nonzero();
    }
}

/// Writes the pages into one PDF with the two Helvetica faces and a title.
pub fn document(pages: Vec<Page>, title: &str, producer: &str) -> Vec<u8> {
    let mut pdf = Pdf::new();
    let catalog = Ref::new(1);
    let tree = Ref::new(2);
    let regular = Ref::new(3);
    let bold = Ref::new(4);
    let info = Ref::new(5);
    let first = 6;
    let page_ids: Vec<Ref> = (0..pages.len() as i32)
        .map(|index| Ref::new(first + index * 2))
        .collect();
    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree)
        .kids(page_ids.iter().copied())
        .count(pages.len() as i32);
    for (font, id) in [(Font::Regular, regular), (Font::Bold, bold)] {
        pdf.type1_font(id)
            .base_font(Name(match font {
                Font::Regular => b"Helvetica",
                Font::Bold => b"Helvetica-Bold",
            }))
            .encoding_predefined(Name(b"WinAnsiEncoding"));
    }
    pdf.document_info(info)
        .title(TextStr(title))
        .producer(TextStr(producer));
    for (page, id) in pages.into_iter().zip(&page_ids) {
        let content = Ref::new(id.get() + 1);
        let mut writer = pdf.page(*id);
        writer
            .parent(tree)
            .media_box(Rect::new(0.0, 0.0, WIDTH, HEIGHT))
            .contents(content);
        writer
            .resources()
            .fonts()
            .pair(Font::Regular.name(), regular)
            .pair(Font::Bold.name(), bold);
        drop(writer);
        pdf.stream(content, &page.content.finish());
    }
    pdf.finish()
}
