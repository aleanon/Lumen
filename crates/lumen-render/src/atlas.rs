//! A GPU glyph atlas allocator (R3.2b).
//!
//! Pure packing logic, independent of any GPU API so it can be unit-tested: the
//! `Wgpu` backend (R3.3) drives it, uploading coverage bitmaps into the texture
//! region each [`GlyphAtlas::alloc`] hands back. Glyphs are packed with a simple
//! **shelf** packer (rows of monotonically-increasing height): cheap, good
//! enough for monospace-ish glyph runs, and stable. Pages are square texture
//! layers; on overflow a new page is added up to `max_pages`, after which the
//! caller [`clear`](GlyphAtlas::clear)s and re-uploads.

use std::collections::HashMap;

/// A packed glyph's location: which `page` (texture layer) and its pixel rect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasSlot {
    /// Page (texture array layer) index.
    pub page: u32,
    /// Left edge in atlas px.
    pub x: u32,
    /// Top edge in atlas px.
    pub y: u32,
    /// Width in px.
    pub w: u32,
    /// Height in px.
    pub h: u32,
}

/// 1px gutter around every glyph so bilinear sampling can't bleed a neighbour.
const PAD: u32 = 1;

struct Shelf {
    y: u32,
    height: u32,
    cursor_x: u32,
}

struct Page {
    shelves: Vec<Shelf>,
    used_y: u32,
}

impl Page {
    fn new() -> Page {
        Page {
            shelves: Vec::new(),
            used_y: 0,
        }
    }

    /// Try to place a `w`×`h` cell on this page, returning its top-left.
    fn place(&mut self, w: u32, h: u32, size: u32) -> Option<(u32, u32)> {
        // Best-fit existing shelf: the shortest shelf that still fits `h` and has
        // horizontal room — keeps tall glyphs from wasting short shelves.
        let mut best: Option<usize> = None;
        for (i, s) in self.shelves.iter().enumerate() {
            let fits = s.height >= h && s.cursor_x + w <= size;
            if fits && best.is_none_or(|b| s.height < self.shelves[b].height) {
                best = Some(i);
            }
        }
        if let Some(i) = best {
            let s = &mut self.shelves[i];
            let x = s.cursor_x;
            let y = s.y;
            s.cursor_x += w;
            return Some((x, y));
        }
        // Open a new shelf at the bottom if there's vertical room.
        if self.used_y + h <= size {
            let y = self.used_y;
            self.used_y += h;
            self.shelves.push(Shelf {
                y,
                height: h,
                cursor_x: w,
            });
            return Some((0, y));
        }
        None
    }
}

/// A multi-page shelf atlas keyed by a stable glyph id (see
/// `lumen_text`'s `GlyphKey::stable_id`).
pub struct GlyphAtlas {
    size: u32,
    max_pages: u32,
    pages: Vec<Page>,
    map: HashMap<u64, AtlasSlot>,
}

impl GlyphAtlas {
    /// A new atlas of square `size`×`size` pages, up to `max_pages` before the
    /// caller must [`clear`](Self::clear).
    pub fn new(size: u32, max_pages: u32) -> GlyphAtlas {
        GlyphAtlas {
            size: size.max(1),
            max_pages: max_pages.max(1),
            pages: Vec::new(),
            map: HashMap::new(),
        }
    }

    /// Page edge length in px.
    pub fn size(&self) -> u32 {
        self.size
    }

    /// Number of pages currently allocated.
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// The slot for `key`, if already packed.
    pub fn get(&self, key: u64) -> Option<AtlasSlot> {
        self.map.get(&key).copied()
    }

    /// Get (or pack) a slot for a `w`×`h` glyph under `key`. Returns `(slot,
    /// fresh)`: when `fresh` is true the slot was newly allocated and the caller
    /// must upload the glyph's pixels into it. Returns `None` if the glyph can't
    /// fit even a fresh page (too large, or `max_pages` exhausted) — the caller
    /// should [`clear`](Self::clear) and retry, or fall back to a sprite.
    pub fn alloc(&mut self, key: u64, w: u32, h: u32) -> Option<(AtlasSlot, bool)> {
        if let Some(&slot) = self.map.get(&key) {
            return Some((slot, false));
        }
        let (cw, ch) = (w + PAD, h + PAD);
        if cw > self.size || ch > self.size {
            return None; // larger than a whole page
        }
        // Existing pages first, then grow.
        for page in 0..self.pages.len() {
            if let Some((x, y)) = self.pages[page].place(cw, ch, self.size) {
                return Some((self.record(key, page as u32, x, y, w, h), true));
            }
        }
        if (self.pages.len() as u32) < self.max_pages {
            let page = self.pages.len();
            self.pages.push(Page::new());
            if let Some((x, y)) = self.pages[page].place(cw, ch, self.size) {
                return Some((self.record(key, page as u32, x, y, w, h), true));
            }
        }
        None
    }

    fn record(&mut self, key: u64, page: u32, x: u32, y: u32, w: u32, h: u32) -> AtlasSlot {
        let slot = AtlasSlot { page, x, y, w, h };
        self.map.insert(key, slot);
        slot
    }

    /// Drop all packed glyphs and pages (eviction). The caller must re-upload any
    /// glyphs it still needs after this.
    pub fn clear(&mut self) {
        self.pages.clear();
        self.map.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedups_by_key() {
        let mut a = GlyphAtlas::new(256, 4);
        let (s1, f1) = a.alloc(7, 10, 12).unwrap();
        let (s2, f2) = a.alloc(7, 10, 12).unwrap();
        assert!(f1 && !f2, "first is fresh, second is a cache hit");
        assert_eq!(s1, s2);
        assert_eq!(a.get(7), Some(s1));
    }

    #[test]
    fn packs_left_to_right_then_new_shelf() {
        let mut a = GlyphAtlas::new(64, 1);
        let (s0, _) = a.alloc(0, 20, 10).unwrap();
        let (s1, _) = a.alloc(1, 20, 10).unwrap();
        assert_eq!((s0.x, s0.y), (0, 0));
        assert_eq!(s1.y, 0, "same shelf");
        assert_eq!(s1.x, 21, "advanced by width + 1px pad");
        // A taller glyph that doesn't fit the remaining width opens a new shelf.
        let (s2, _) = a.alloc(2, 40, 30).unwrap();
        assert!(s2.y > 0, "new shelf below the first (got y={})", s2.y);
    }

    #[test]
    fn grows_to_a_new_page_then_overflows() {
        // 16px pages fit one ~14px glyph each (plus pad), so each glyph needs a page.
        let mut a = GlyphAtlas::new(16, 2);
        let (p0, _) = a.alloc(0, 14, 14).unwrap();
        let (p1, _) = a.alloc(1, 14, 14).unwrap();
        assert_eq!(p0.page, 0);
        assert_eq!(p1.page, 1, "second glyph spilled to a new page");
        assert_eq!(a.page_count(), 2);
        // Third can't fit: pages exhausted.
        assert!(a.alloc(2, 14, 14).is_none());
        // After eviction it packs again from scratch.
        a.clear();
        assert_eq!(a.page_count(), 0);
        let (p2, fresh) = a.alloc(2, 14, 14).unwrap();
        assert!(fresh && p2.page == 0);
    }

    #[test]
    fn rejects_glyphs_larger_than_a_page() {
        let mut a = GlyphAtlas::new(32, 4);
        assert!(a.alloc(0, 40, 10).is_none());
        assert!(a.alloc(1, 10, 40).is_none());
    }
}
