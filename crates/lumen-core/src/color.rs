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
        let r = linear_to_srgb8(self.r);
        let g = linear_to_srgb8(self.g);
        let b = linear_to_srgb8(self.b);
        let a = (self.a.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
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
}
