//! [`RgbaImage`] — a straight (non-premultiplied) RGBA8 raster.
//!
//! This is Lumen's own image type (the `image` crate is not on the ADR-003
//! whitelist; see decision log). PNG encode/decode go through tiny-skia's
//! `png-format` feature, and it is the return type of `Headless::screenshot`
//! (02 §8) and the format goldens are stored in.

use tiny_skia::{ColorU8, Pixmap};

/// An 8-bit-per-channel, straight-alpha RGBA image, row-major.
#[derive(Clone, PartialEq, Eq)]
pub struct RgbaImage {
    width: u32,
    height: u32,
    /// `width * height * 4` bytes, R,G,B,A per pixel, top-to-bottom.
    pixels: Vec<u8>,
}

impl RgbaImage {
    /// A fully transparent image of the given size.
    pub fn new(width: u32, height: u32) -> RgbaImage {
        RgbaImage {
            width,
            height,
            pixels: vec![0; (width as usize) * (height as usize) * 4],
        }
    }

    /// Construct from raw straight-alpha RGBA8 bytes. Panics if the length is
    /// not `width * height * 4`.
    pub fn from_raw(width: u32, height: u32, pixels: Vec<u8>) -> RgbaImage {
        assert_eq!(pixels.len(), (width as usize) * (height as usize) * 4);
        RgbaImage {
            width,
            height,
            pixels,
        }
    }

    /// Image width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Image height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The raw straight-alpha RGBA8 bytes.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Build from a tiny-skia pixmap, un-premultiplying each pixel.
    pub(crate) fn from_pixmap(pm: &Pixmap) -> RgbaImage {
        let mut pixels = Vec::with_capacity(pm.data().len());
        for px in pm.pixels() {
            let c = px.demultiply();
            pixels.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
        }
        RgbaImage {
            width: pm.width(),
            height: pm.height(),
            pixels,
        }
    }

    /// Build a tiny-skia pixmap from this image (premultiplying each pixel).
    pub(crate) fn to_pixmap(&self) -> Pixmap {
        let mut pm = Pixmap::new(self.width, self.height).expect("valid pixmap size");
        for (dst, src) in pm.pixels_mut().iter_mut().zip(self.pixels.chunks_exact(4)) {
            *dst = ColorU8::from_rgba(src[0], src[1], src[2], src[3]).premultiply();
        }
        pm
    }

    /// Encode to PNG bytes (via tiny-skia's `png-format`).
    pub fn to_png(&self) -> Vec<u8> {
        self.to_pixmap().encode_png().expect("PNG encode")
    }

    /// Decode from PNG bytes.
    pub fn from_png(bytes: &[u8]) -> Result<RgbaImage, String> {
        let pm = Pixmap::decode_png(bytes).map_err(|e| e.to_string())?;
        Ok(RgbaImage::from_pixmap(&pm))
    }

    /// A copy of the `[x, y, w, h]` sub-rectangle (clamped to bounds).
    pub fn crop(&self, x: u32, y: u32, w: u32, h: u32) -> RgbaImage {
        let w = w.min(self.width.saturating_sub(x));
        let h = h.min(self.height.saturating_sub(y));
        let mut pixels = Vec::with_capacity((w as usize) * (h as usize) * 4);
        for row in 0..h {
            let sy = (y + row) as usize;
            let start = (sy * self.width as usize + x as usize) * 4;
            let end = start + (w as usize) * 4;
            pixels.extend_from_slice(&self.pixels[start..end]);
        }
        RgbaImage {
            width: w,
            height: h,
            pixels,
        }
    }

    /// Count of pixels that differ from `other` (images must be the same size).
    pub fn diff_count(&self, other: &RgbaImage) -> usize {
        if self.width != other.width || self.height != other.height {
            return usize::MAX;
        }
        self.pixels
            .chunks_exact(4)
            .zip(other.pixels.chunks_exact(4))
            .filter(|(a, b)| a != b)
            .count()
    }
}

impl std::fmt::Debug for RgbaImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RgbaImage({}x{})", self.width, self.height)
    }
}
