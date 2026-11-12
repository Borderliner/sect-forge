use std::{cell::RefCell, collections::HashMap, path::Path, sync::Arc};

use gpui::RenderImage;
use image::{Frame, RgbaImage};

use super::PAPER;
use crate::game;

const GLYPH_MAP: usize = 32;
const GLYPH_METRICS: usize = GLYPH_MAP + (1 << 16) * 2;
const METRIC_SIZE: usize = 5;

pub(super) struct GameHeadingFont {
    data: Vec<u8>,
    em: f32,
    offsets: usize,
    bitmap: usize,
    bitmap_end: usize,
    images: RefCell<HashMap<(String, u16), Heading>>,
}

#[derive(Clone)]
pub(super) struct Heading {
    pub image: Arc<RenderImage>,
    pub width_em: f32,
    pub height_em: f32,
}

impl GameHeadingFont {
    pub fn load(game_path: &Path) -> Option<Self> {
        let pack = game::open_pack(&game_path.join("data/local_en.pack")).ok()?;
        ["font/iskra-bold_84.cuf", "font/iskra-bold_64.cuf"]
            .iter()
            .find_map(|path| Self::decode(game::raw(&pack, path).ok()?))
    }

    fn decode(data: Vec<u8>) -> Option<Self> {
        if !data.starts_with(b"CUF0") {
            return None;
        }
        // Three Kingdoms stores five metric bytes and a separate bitmap offset per glyph.
        let em = f32::from(read_u16(&data, 8)?);
        let count = usize::from(read_u16(&data, 26)?);
        let offsets = GLYPH_METRICS + count * METRIC_SIZE;
        let bitmap = offsets + count * 4;
        let bitmap_end = bitmap.checked_add(read_u32(&data, 28)? as usize)?;
        if em == 0. || count == 0 || bitmap_end > data.len() {
            return None;
        }
        Some(Self {
            data,
            em,
            offsets,
            bitmap,
            bitmap_end,
            images: RefCell::default(),
        })
    }

    pub fn heading(&self, text: &str, pixel_em: u16) -> Option<Heading> {
        let key = (text.to_owned(), pixel_em);
        if let Some(image) = self.images.borrow().get(&key) {
            return Some(image.clone());
        }
        let heading = self.rasterize(text, pixel_em)?;
        let mut images = self.images.borrow_mut();
        if images.len() >= 96 {
            images.clear();
        }
        images.insert(key, heading.clone());
        Some(heading)
    }

    fn rasterize(&self, text: &str, pixel_em: u16) -> Option<Heading> {
        if text.len() > 128 {
            return None;
        }
        let mut glyphs = Vec::new();
        let mut advance = 0;
        let mut left = 0;
        let mut right = 0;
        let mut top = i32::MAX;
        let mut bottom = i32::MIN;
        for character in text.chars() {
            let code = u16::try_from(u32::from(character)).ok()?;
            let glyph = usize::from(read_u16(&self.data, GLYPH_MAP + usize::from(code) * 2)?);
            if glyph == 0 && !character.is_whitespace() {
                return None;
            }
            let metrics = GLYPH_METRICS + glyph * METRIC_SIZE;
            let [y, x, width, height, step] = self.data[..self.offsets]
                .get(metrics..metrics + METRIC_SIZE)?
                .try_into()
                .ok()?;
            let x = advance + i32::from(x as i8);
            let y = i32::from(y as i8);
            advance += i32::from(step);
            if character.is_whitespace() {
                continue;
            }
            let offset = self
                .bitmap
                .checked_add(read_u32(&self.data, self.offsets + glyph * 4)? as usize)?;
            let len = usize::from(width) * usize::from(height);
            let alpha = self
                .data
                .get(..self.bitmap_end)?
                .get(offset..offset.checked_add(len)?)?;
            left = left.min(x);
            right = right.max(x + i32::from(width));
            top = top.min(y);
            bottom = bottom.max(y + i32::from(height));
            glyphs.push((x, y, usize::from(width), alpha));
        }
        if glyphs.is_empty() || bottom <= top {
            return None;
        }
        let width = (right.max(advance) - left) as usize;
        let height = (bottom - top) as usize;
        let mut mask = vec![0; width * height];
        for (x, y, stride, alpha) in glyphs {
            if stride == 0 {
                continue;
            }
            for (row, coverage) in alpha.chunks_exact(stride).enumerate() {
                let start = ((y - top) as usize + row) * width + (x - left) as usize;
                for (column, &alpha) in coverage.iter().enumerate() {
                    let pixel = &mut mask[start + column];
                    *pixel = (*pixel).max(alpha);
                }
            }
        }
        let scale = (f32::from(pixel_em) / self.em).min(1.);
        let target_width = ((width as f32 * scale).round() as usize).max(1);
        let target_height = ((height as f32 * scale).round() as usize).max(1);
        let coverage = resize_coverage(&mask, width, height, target_width, target_height);
        let [_, r, g, b] = PAPER.to_be_bytes();
        // GPUI's render images take BGRA bytes. Pass the pixels directly so
        // the first frame never waits on the asynchronous image decoder.
        let bgra = coverage
            .into_iter()
            .flat_map(|alpha| [b, g, r, alpha])
            .collect::<Vec<_>>();
        let pixels = RgbaImage::from_raw(target_width as u32, target_height as u32, bgra)?;
        Some(Heading {
            image: Arc::new(RenderImage::new([Frame::new(pixels)])),
            width_em: width as f32 / self.em,
            height_em: height as f32 / self.em,
        })
    }
}

// Average the source coverage over each device pixel. Sampling a large bitmap
// directly at the final UI size can miss strokes and leave staircase edges.
fn resize_coverage(
    mask: &[u8],
    width: usize,
    height: usize,
    out_w: usize,
    out_h: usize,
) -> Vec<u8> {
    if width == out_w && height == out_h {
        return mask.to_vec();
    }
    let step_x = width as f32 / out_w as f32;
    let step_y = height as f32 / out_h as f32;
    let mut output = Vec::with_capacity(out_w * out_h);
    for y in 0..out_h {
        let y0 = y as f32 * step_y;
        let y1 = y0 + step_y;
        for x in 0..out_w {
            let x0 = x as f32 * step_x;
            let x1 = x0 + step_x;
            let mut sum = 0.;
            for row in y0 as usize..(y1.ceil() as usize).min(height) {
                let overlap_y = y1.min(row as f32 + 1.) - y0.max(row as f32);
                for col in x0 as usize..(x1.ceil() as usize).min(width) {
                    let overlap_x = x1.min(col as f32 + 1.) - x0.max(col as f32);
                    sum += f32::from(mask[row * width + col]) * overlap_y * overlap_x;
                }
            }
            output.push((sum / (step_x * step_y)).round() as u8);
        }
    }
    output
}

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        data.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::resize_coverage;

    #[test]
    fn downsampling_preserves_partial_glyph_coverage() {
        let mask = [0, 255, 0, 255, 255, 255];
        assert_eq!(resize_coverage(&mask, 3, 2, 1, 1), vec![170]);
        assert_eq!(resize_coverage(&mask, 3, 2, 3, 2), mask);
    }
}
