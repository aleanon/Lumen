//! Color.
//!
//! [`Color`] is `f32` RGBA stored **linear-light** internally, with sRGB at the
//! API boundary (02 §1). Storing linear means compositing, gradients (which
//! interpolate in Oklab, ADR-017), and the perceptual diff metric all operate
//! in a physically meaningful space; the sRGB transfer function is applied only
//! when converting to/from 8-bit boundary values.
//!
//! Alpha is linear (not gamma-encoded), matching standard practice.

/// An RGBA color, stored as linear-light `f32` components, each nominally in
/// `[0, 1]` (values outside that range are permitted for HDR/intermediate math).
///
/// ```
/// use lumen_core::Color;
/// let c = Color::from_hex("#1a73e8ff").unwrap();
/// assert_eq!(c.to_hex(), "#1a73e8ff");
/// // Construction from sRGB bytes round-trips through the canonical hex form.
/// assert_eq!(Color::srgb8(26, 115, 232, 255).to_hex(), "#1a73e8ff");
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    /// Linear-light red.
    pub r: f32,
    /// Linear-light green.
    pub g: f32,
    /// Linear-light blue.
    pub b: f32,
    /// Linear alpha in `[0, 1]`.
    pub a: f32,
}

impl Color {
    /// Fully transparent (all components zero).
    pub const TRANSPARENT: Color = Color::new_linear(0.0, 0.0, 0.0, 0.0);
    /// Opaque black.
    pub const BLACK: Color = Color::new_linear(0.0, 0.0, 0.0, 1.0);
    /// Opaque white (linear 1.0 == sRGB white).
    pub const WHITE: Color = Color::new_linear(1.0, 1.0, 1.0, 1.0);

    /// Construct directly from linear-light components. Prefer [`Color::srgb8`]
    /// or [`Color::from_hex`] for author-facing values.
    pub const fn new_linear(r: f32, g: f32, b: f32, a: f32) -> Color {
        Color { r, g, b, a }
    }

    /// Construct from 8-bit **sRGB** channels plus 8-bit alpha. The RGB channels
    /// are decoded through the sRGB transfer function to linear; alpha is scaled
    /// linearly.
    pub fn srgb8(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color {
            r: srgb_to_linear(r as f32 / 255.0),
            g: srgb_to_linear(g as f32 / 255.0),
            b: srgb_to_linear(b as f32 / 255.0),
            a: a as f32 / 255.0,
        }
    }

    /// Parse a hex color: `#rgb`, `#rgba`, `#rrggbb`, or `#rrggbbaa` (the
    /// leading `#` is optional). Shorthand digits are expanded by duplication
    /// (`#f80` → `#ff8800`).
    pub fn from_hex(s: &str) -> Result<Color, ColorParseError> {
        let h = s.strip_prefix('#').unwrap_or(s);
        if !h.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(ColorParseError);
        }
        let (r, g, b, a) = match h.len() {
            3 => {
                let v = parse_short(h)?;
                (v.0, v.1, v.2, 255)
            }
            4 => parse_short4(h)?,
            6 => (hex2(h, 0)?, hex2(h, 2)?, hex2(h, 4)?, 255),
            8 => (hex2(h, 0)?, hex2(h, 2)?, hex2(h, 4)?, hex2(h, 6)?),
            _ => return Err(ColorParseError),
        };
        Ok(Color::srgb8(r, g, b, a))
    }

    /// Serialize to the canonical `#rrggbbaa` form used by `ui.getStyles` and
    /// test assertions (04 §7). RGB channels are encoded back to sRGB.
    pub fn to_hex(&self) -> String {
        let [r, g, b, a] = self.to_srgb8();
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }

    /// The color as 8-bit **sRGB** RGBA — the boundary representation handed to
    /// the rasterizer (which works in gamma space).
    pub fn to_srgb8(&self) -> [u8; 4] {
        [
            linear_to_srgb8(self.r),
            linear_to_srgb8(self.g),
            linear_to_srgb8(self.b),
            (self.a.clamp(0.0, 1.0) * 255.0).round() as u8,
        ]
    }

    /// Interpolate toward `other` by `t` in `[0, 1]`, **in Oklab** (ADR-017),
    /// which matches the perceptual diff metric and gives even-looking ramps.
    /// Alpha interpolates linearly.
    pub fn lerp_oklab(self, other: Color, t: f32) -> Color {
        let (l1, a1, b1) = linear_to_oklab(self.r, self.g, self.b);
        let (l2, a2, b2) = linear_to_oklab(other.r, other.g, other.b);
        let l = l1 + (l2 - l1) * t;
        let a = a1 + (a2 - a1) * t;
        let b = b1 + (b2 - b1) * t;
        let (r, g, bl) = oklab_to_linear(l, a, b);
        Color {
            r,
            g,
            b: bl,
            a: self.a + (other.a - self.a) * t,
        }
    }

    /// Perceptual color difference (ΔE) in Oklab — the metric used for GPU↔CPU
    /// parity (05 §4). Alpha is ignored.
    pub fn delta_e_oklab(self, other: Color) -> f32 {
        let (l1, a1, b1) = linear_to_oklab(self.r, self.g, self.b);
        let (l2, a2, b2) = linear_to_oklab(other.r, other.g, other.b);
        ((l1 - l2).powi(2) + (a1 - a2).powi(2) + (b1 - b2).powi(2)).sqrt()
    }
}

/// Linear sRGB → Oklab (Björn Ottosson's matrices).
fn linear_to_oklab(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let l = 0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b;
    let m = 0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b;
    let s = 0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b;
    let l_ = l.cbrt();
    let m_ = m.cbrt();
    let s_ = s.cbrt();
    (
        0.210_454_26 * l_ + 0.793_617_8 * m_ - 0.004_072_047 * s_,
        1.977_998_5 * l_ - 2.428_592_2 * m_ + 0.450_593_7 * s_,
        0.025_904_037 * l_ + 0.782_771_77 * m_ - 0.808_675_77 * s_,
    )
}

/// Oklab → linear sRGB (inverse of [`linear_to_oklab`]).
fn oklab_to_linear(l: f32, a: f32, b: f32) -> (f32, f32, f32) {
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;
    (
        4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s,
        -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s,
        -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s,
    )
}

impl std::fmt::Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// Error parsing a hex color string.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ColorParseError;

impl std::fmt::Display for ColorParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid hex color (expected #rgb, #rgba, #rrggbb, or #rrggbbaa)")
    }
}

impl std::error::Error for ColorParseError {}

fn hex2(s: &str, at: usize) -> Result<u8, ColorParseError> {
    u8::from_str_radix(&s[at..at + 2], 16).map_err(|_| ColorParseError)
}

fn hex1(c: u8) -> u8 {
    // duplicate nibble: 0xf -> 0xff
    c * 17
}

fn parse_short(h: &str) -> Result<(u8, u8, u8), ColorParseError> {
    let b = h.as_bytes();
    Ok((
        hex1(nibble(b[0])?),
        hex1(nibble(b[1])?),
        hex1(nibble(b[2])?),
    ))
}

fn parse_short4(h: &str) -> Result<(u8, u8, u8, u8), ColorParseError> {
    let b = h.as_bytes();
    Ok((
        hex1(nibble(b[0])?),
        hex1(nibble(b[1])?),
        hex1(nibble(b[2])?),
        hex1(nibble(b[3])?),
    ))
}

fn nibble(c: u8) -> Result<u8, ColorParseError> {
    (c as char)
        .to_digit(16)
        .map(|d| d as u8)
        .ok_or(ColorParseError)
}

/// sRGB electro-optical transfer function (gamma decode), per IEC 61966-2-1.
fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.040_448_237 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Inverse sRGB transfer (gamma encode), then quantize to 8 bits with
/// round-to-nearest. Round-trips all 256 byte values (see tests).
fn linear_to_srgb8(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let encoded = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb8_roundtrips_all_byte_values() {
        // Every 8-bit gray level must survive srgb8 -> linear -> srgb8 so that
        // the canonical hex form is stable (golden tests depend on it).
        for v in 0u8..=255 {
            let c = Color::srgb8(v, v, v, 255);
            assert_eq!(
                linear_to_srgb8(c.r),
                v,
                "channel value {v} did not round-trip"
            );
        }
    }

    #[test]
    fn hex_parse_forms() {
        assert_eq!(Color::from_hex("#fff").unwrap(), Color::WHITE);
        assert_eq!(Color::from_hex("ffffffff").unwrap(), Color::WHITE);
        assert_eq!(Color::from_hex("#000000ff").unwrap(), Color::BLACK);
        assert_eq!(Color::from_hex("#f80").unwrap().to_hex(), "#ff8800ff");
        assert_eq!(Color::from_hex("#1a73e8ff").unwrap().to_hex(), "#1a73e8ff");
        assert!(Color::from_hex("#xyz").is_err());
        assert!(Color::from_hex("#12345").is_err());
    }

    #[test]
    fn srgb_white_and_black_are_exact() {
        assert_eq!(Color::srgb8(255, 255, 255, 255), Color::WHITE);
        assert_eq!(Color::srgb8(0, 0, 0, 255), Color::BLACK);
    }

    #[test]
    fn alpha_is_linear() {
        let c = Color::srgb8(0, 0, 0, 128);
        assert!((c.a - 128.0 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn oklab_lerp_reproduces_endpoints() {
        let a = Color::srgb8(255, 0, 0, 255);
        let b = Color::srgb8(0, 0, 255, 255);
        assert_eq!(a.lerp_oklab(b, 0.0).to_srgb8(), a.to_srgb8());
        assert_eq!(a.lerp_oklab(b, 1.0).to_srgb8(), b.to_srgb8());
        // midpoint stays in gamut and is distinct from both ends
        let mid = a.lerp_oklab(b, 0.5).to_srgb8();
        assert_ne!(mid, a.to_srgb8());
        assert_ne!(mid, b.to_srgb8());
    }
}
