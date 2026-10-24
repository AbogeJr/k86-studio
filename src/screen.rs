//! 240x135 screen images: import, frame model and the wire encoding
//! (RGB565 big-endian, column-major, cropped to the non-black bounding box).

use std::path::Path;

use anyhow::{Context, Result};
use image::{AnimationDecoder, Rgba, RgbaImage};

pub const W: u32 = 240;
pub const H: u32 = 135;
/// Frame cap from the original editor (7 MiB flash, 64 KiB per frame, 5 static slots).
pub const MAX_FRAMES: usize = 106;
pub const SLOTS: u8 = 5;
/// The eraser paints rgb(0,0,1): visually black but not "transparent" for the bbox.
pub const ERASE: [u8; 3] = [0, 0, 1];

pub type Frame = RgbaImage;

pub fn blank() -> Frame {
    RgbaImage::from_pixel(W, H, Rgba([0, 0, 0, 255]))
}

/// Source pictures as decoded from a file, before placement on the screen.
pub struct Source {
    pub frames: Vec<RgbaImage>,
    /// Average per-frame delay in ms (`min(cs*10, 255)` averaged), if animated.
    pub delay_ms: Option<u8>,
}

pub fn load(path: &Path) -> Result<Source> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let format = image::guess_format(&bytes).context("unrecognised image format")?;
    if format == image::ImageFormat::Gif {
        let dec = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(&bytes))?;
        let frames = dec.into_frames().collect_frames()?;
        if frames.len() > 1 {
            let delays: Vec<u32> = frames
                .iter()
                .map(|f| {
                    let (n, d) = f.delay().numer_denom_ms();
                    (n / d.max(1)).min(255)
                })
                .collect();
            let avg = (delays.iter().sum::<u32>() as f32 / delays.len() as f32).round() as u8;
            return Ok(Source {
                frames: frames.into_iter().take(MAX_FRAMES).map(|f| f.into_buffer()).collect(),
                delay_ms: Some(avg),
            });
        }
    }
    let img = image::load_from_memory(&bytes)?.to_rgba8();
    Ok(Source { frames: vec![img], delay_ms: None })
}

/// "Contain" fit used by the original (`calculateAutoSize`): (x, y, w, h).
pub fn fit(src_w: u32, src_h: u32) -> (f32, f32, f32, f32) {
    let (sw, sh) = (src_w as f32, src_h as f32);
    let (w, h) = if (W as f32 / H as f32) < sw / sh {
        (W as f32, (W as f32 * sh / sw).ceil())
    } else {
        ((H as f32 * sw / sh).ceil(), H as f32)
    };
    (((W as f32 - w) / 2.0).round(), ((H as f32 - h) / 2.0).round(), w, h)
}

/// Draws `src` onto a black 240x135 frame at `offset` with `scale` applied to the fitted size.
pub fn place(src: &RgbaImage, offset: (f32, f32), scale: f32) -> Frame {
    let (_, _, fw, fh) = fit(src.width(), src.height());
    let (w, h) = ((fw * scale).round().max(1.0) as u32, (fh * scale).round().max(1.0) as u32);
    let resized = image::imageops::resize(src, w, h, image::imageops::FilterType::Triangle);
    let mut out = blank();
    for (x, y, p) in resized.enumerate_pixels() {
        let (dx, dy) = (x as i64 + offset.0 as i64, y as i64 + offset.1 as i64);
        if dx < 0 || dy < 0 || dx >= W as i64 || dy >= H as i64 {
            continue;
        }
        // Composite over black; alpha is otherwise ignored by the keyboard.
        let a = p[3] as u16;
        let c = |v: u8| ((v as u16 * a) / 255) as u8;
        out.put_pixel(dx as u32, dy as u32, Rgba([c(p[0]), c(p[1]), c(p[2]), 255]));
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rect {
    pub left: u16,
    pub top: u16,
    pub right: u16,
    pub bottom: u16,
}

impl Rect {
    fn union(self, o: Rect) -> Rect {
        Rect {
            left: self.left.min(o.left),
            top: self.top.min(o.top),
            right: self.right.max(o.right),
            bottom: self.bottom.max(o.bottom),
        }
    }
}

/// Bounding box of non-pure-black pixels, right/bottom exclusive (`calculateDataPos`).
pub fn bbox(f: &Frame) -> Rect {
    let (mut l, mut t, mut r, mut b) = (u32::MAX, u32::MAX, 0, 0);
    let mut any = false;
    for (x, y, p) in f.enumerate_pixels() {
        if p[0] | p[1] | p[2] != 0 {
            any = true;
            l = l.min(x);
            t = t.min(y);
            r = r.max(x + 1);
            b = b.max(y + 1);
        }
    }
    if !any {
        return Rect::default();
    }
    Rect { left: l as u16, top: t as u16, right: r as u16, bottom: b as u16 }
}

pub fn union_bbox(frames: &[Frame]) -> Rect {
    let mut boxes = frames.iter().map(bbox);
    let first = boxes.next().unwrap_or_default();
    boxes.fold(first, Rect::union)
}

/// RGB565 big-endian, column-major within `r`.
pub fn encode(f: &Frame, r: Rect) -> Vec<u8> {
    let mut out = Vec::with_capacity((r.right - r.left) as usize * (r.bottom - r.top) as usize * 2);
    for x in r.left..r.right {
        for y in r.top..r.bottom {
            let p = f.get_pixel(x as u32, y as u32);
            let v = ((p[0] as u16 >> 3) << 11) | ((p[1] as u16 >> 2) << 5) | (p[2] as u16 >> 3);
            out.push((v >> 8) as u8);
            out.push(v as u8);
        }
    }
    out
}

pub fn save_png(path: &Path, f: &Frame) -> Result<()> {
    f.save(path)?;
    Ok(())
}

pub fn save_gif(path: &Path, frames: &[Frame], delay_ms: u16) -> Result<()> {
    let file = std::fs::File::create(path)?;
    let mut enc = image::codecs::gif::GifEncoder::new(file);
    enc.set_repeat(image::codecs::gif::Repeat::Infinite)?;
    let delay = image::Delay::from_numer_denom_ms(delay_ms.max(10) as u32, 1);
    for f in frames {
        enc.encode_frame(image::Frame::from_parts(f.clone(), 0, 0, delay))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_frame_geometry() {
        let f = RgbaImage::from_pixel(W, H, Rgba([255, 255, 255, 255]));
        let r = bbox(&f);
        assert_eq!(r, Rect { left: 0, top: 0, right: 240, bottom: 135 });
        let d = encode(&f, r);
        assert_eq!(d.len(), 64_800);
        assert_eq!(&d[..2], &[0xFF, 0xFF]);
    }

    #[test]
    fn column_major_big_endian() {
        let mut f = blank();
        f.put_pixel(10, 5, Rgba([255, 0, 0, 255]));
        f.put_pixel(10, 6, Rgba([0, 0, 255, 255]));
        f.put_pixel(11, 5, Rgba([0, 255, 0, 255]));
        let r = bbox(&f);
        assert_eq!(r, Rect { left: 10, top: 5, right: 12, bottom: 7 });
        // x=10: (10,5) red, (10,6) blue; x=11: (11,5) green, (11,6) black.
        assert_eq!(encode(&f, r), vec![0xF8, 0x00, 0x00, 0x1F, 0x07, 0xE0, 0x00, 0x00]);
    }

    #[test]
    fn black_frame_is_empty() {
        assert_eq!(bbox(&blank()), Rect::default());
    }

    #[test]
    fn contain_fit() {
        assert_eq!(fit(480, 270), (0.0, 0.0, 240.0, 135.0));
        assert_eq!(fit(100, 100), (53.0, 0.0, 135.0, 135.0));
    }
}
