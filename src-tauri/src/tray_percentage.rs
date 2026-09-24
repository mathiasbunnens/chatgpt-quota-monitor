//! Fixed contrast numeric badge: readable against light and dark taskbars.
pub fn icon(value: Option<u32>) -> tauri::image::Image<'static> {
    let glyphs: [[u8; 5]; 11] = [
        [7, 5, 5, 5, 7],
        [2, 6, 2, 2, 7],
        [7, 1, 7, 4, 7],
        [7, 1, 7, 1, 7],
        [5, 5, 7, 1, 1],
        [7, 4, 7, 1, 7],
        [7, 4, 7, 5, 7],
        [7, 1, 1, 1, 1],
        [7, 5, 7, 5, 7],
        [7, 5, 7, 1, 7],
        [0, 0, 7, 0, 0],
    ];
    let text = value
        .map(|n| n.min(100).to_string())
        .unwrap_or_else(|| "--".into());
    let mut rgba = vec![0; 32 * 32 * 4];
    for y in 1..31 {
        for x in 1..31 {
            let i = (y * 32 + x) * 4;
            rgba[i..i + 4].copy_from_slice(&[28, 28, 30, 255]);
        }
    }
    let mut pixel = |x: usize, y: usize| {
        let i = (y * 32 + x) * 4;
        rgba[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
    };
    let scale = if text.len() < 3 { 4 } else { 2 };
    let height_scale = 5;
    let advance = 3 * scale + 2;
    let width = text.len() * advance - 2;
    let start = (32 - width) / 2;
    for (index, ch) in text.chars().enumerate() {
        let glyph = glyphs[ch.to_digit(10).unwrap_or(10) as usize];
        for (y, row) in glyph.iter().enumerate() {
            for x in 0..3 {
                if row & (1 << (2 - x)) != 0 {
                    for dy in 0..height_scale {
                        for dx in 0..scale {
                            pixel(
                                start + index * advance + x * scale + dx,
                                3 + y * height_scale + dy,
                            );
                        }
                    }
                }
            }
        }
    }
    tauri::image::Image::new_owned(rgba, 32, 32)
}
#[cfg(test)]
mod tests {
    #[test]
    fn badge_handles_all_values() {
        for n in [
            None,
            Some(0),
            Some(9),
            Some(10),
            Some(99),
            Some(100),
            Some(101),
        ] {
            let image = super::icon(n);
            assert_eq!(image.rgba().len(), 32 * 32 * 4);
            assert!(image
                .rgba()
                .chunks_exact(4)
                .any(|p| p == [255, 255, 255, 255]));
        }
        assert_ne!(super::icon(Some(0)).rgba(), super::icon(Some(100)).rgba());
    }
}
