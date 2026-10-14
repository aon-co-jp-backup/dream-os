//! 同梱の日本語フォントの検査: 字形の網羅を確かめ、フォントの書き出し先を表示する(`font_check [出力フォルダ]`)。
use dream_os_fonts::{missing, IPA_GOTHIC, SAMPLE};

fn main() {
    let miss = missing(SAMPLE);
    println!("font: IPAGothic (IPA Font License v1.0), {} bytes", IPA_GOTHIC.len());
    println!("sample chars: {}, missing glyphs: {}", SAMPLE.chars().count(), miss.len());
    if let Some(dir) = std::env::args().nth(1) {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(std::path::Path::new(&dir).join("ipag.ttf"), IPA_GOTHIC).unwrap();
        println!("wrote {dir}/ipag.ttf");
    }
    if miss.is_empty() {
        println!("result: OK (Japanese text such as 「今日は、様です。」 is fully covered)");
    } else {
        println!("result: MISSING {miss:?}");
        std::process::exit(1);
    }
}
