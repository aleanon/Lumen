//! Item 3: pixel probes on RgbaImage — pixel() and region_is_uniform().

use lumen_render::RgbaImage;

#[test]
fn pixel_reads_and_bounds_check() {
    // 2×2, all white except pixel (1,1) black.
    let mut px = vec![255u8; 16];
    let i = 12; // pixel (x=1,y=1) in a 2-wide image: (y*w+x)*4
    px[i..i + 4].copy_from_slice(&[0, 0, 0, 255]);
    let img = RgbaImage::from_raw(2, 2, px);
    assert_eq!(img.pixel(0, 0), [255, 255, 255, 255]);
    assert_eq!(img.pixel(1, 1), [0, 0, 0, 255]);
    assert_eq!(img.pixel(9, 9), [0, 0, 0, 0], "out of bounds → transparent");
}

#[test]
fn region_is_uniform_detects_content() {
    let mut px = vec![255u8; 16];
    let i = 12; // pixel (x=1,y=1) in a 2-wide image: (y*w+x)*4
    px[i..i + 4].copy_from_slice(&[0, 0, 0, 255]);
    let img = RgbaImage::from_raw(2, 2, px);
    assert_eq!(
        img.region_is_uniform(0, 0, 1, 1),
        Some([255, 255, 255, 255])
    );
    assert_eq!(
        img.region_is_uniform(0, 0, 2, 2),
        None,
        "region spanning the black pixel is not uniform"
    );
    assert_eq!(
        img.region_is_uniform(5, 5, 2, 2),
        None,
        "empty region → None"
    );
}
