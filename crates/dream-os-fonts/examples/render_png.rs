//! 同梱フォントで日本語を描いて PNG に保存する: `render_png <出力.png> [文字列] [px]`
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let out = a.get(1).cloned().unwrap_or_else(|| "dream-os-font.png".into());
    let text = a.get(2).cloned().unwrap_or_else(|| "今日は、様です。ひらがな カタカナ 漢字変換 般若心経 ABC 123".into());
    let px: f32 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(48.0);
    let (w, h, img) = dream_os_fonts::render_text(&text, px);
    // 白地に黒の文字にする
    let gray: Vec<u8> = img.iter().map(|&p| 255 - p).collect();
    let file = std::fs::File::create(&out).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
    enc.set_color(png::ColorType::Grayscale);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&gray).unwrap();
    println!("wrote {out} ({w}x{h})");
}
