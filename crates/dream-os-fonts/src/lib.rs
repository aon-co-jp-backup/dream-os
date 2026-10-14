//! dream-os に同梱する日本語フォント(IPAゴシック、IPA Font License v1.0、`fonts/IPA-LICENSE.txt`)。
//! ひらがな・カタカナ・JIS第一・第二水準漢字の字形を持つことを、`coverage` で確かめられる。

/// IPAゴシックの TrueType フォント本体。
pub const IPA_GOTHIC: &[u8] = include_bytes!("../fonts/ipag.ttf");

/// フォントに、その文字の字形があるか。
pub fn has_glyph(c: char) -> bool {
    ttf_parser::Face::parse(IPA_GOTHIC, 0).ok().and_then(|f| f.glyph_index(c)).is_some()
}

/// 文字列のうち、字形が無い文字(空白・制御文字を除く)。
pub fn missing(text: &str) -> Vec<char> {
    let face = match ttf_parser::Face::parse(IPA_GOTHIC, 0) {
        Ok(f) => f,
        Err(_) => return text.chars().collect(),
    };
    text.chars().filter(|c| !c.is_whitespace() && !c.is_control() && face.glyph_index(*c).is_none()).collect()
}

/// 検査用の見本: かな全部・常用漢字の一部・記号・半角全角。
pub const SAMPLE: &str = "あいうえおかきくけこさしすせそたちつてとなにぬねのはひふへほまみむめもやゆよらりるれろわをんがぎぐげござじずぜぞだぢづでどばびぶべぼぱぴぷぺぽぁぃぅぇぉっゃゅょ\
アイウエオカキクケコサシスセソタチツテトナニヌネノハヒフヘホマミムメモヤユヨラリルレロワヲンガギグゲゴザジズゼゾダヂヅデドバビブベボパピプペポァィゥェォッャュョヴー\
日本語漢字変換今日様用要陽洋葉容鬼滅刃経心般若波羅蜜多時観自在菩薩深行照見五蘊皆空度一切苦厄舎利子色不異空即是受想識亦復如東京大阪名古屋札幌福岡仙台広島神戸京都\
、。・「」『』（）［］｛｝〜～！？：；＋－＝／＼￥＠＃＄％＆＊＾＿｜ＡＢＣａｂｃ０１２３ABCabc0123";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn japanese_sample_is_fully_covered() {
        assert!(missing(SAMPLE).is_empty(), "字形が無い文字: {:?}", missing(SAMPLE));
    }

    #[test]
    fn font_is_a_valid_ttf_and_has_many_glyphs() {
        let f = ttf_parser::Face::parse(IPA_GOTHIC, 0).unwrap();
        assert!(f.number_of_glyphs() > 6000, "{}", f.number_of_glyphs());
    }

    #[test]
    fn missing_reports_a_character_the_font_lacks() {
        assert_eq!(missing("あ😀"), vec!['😀']);
        assert!(has_glyph('漢') && !has_glyph('😀'));
    }

    #[test]
    fn license_file_is_bundled() {
        let l = include_str!("../fonts/IPA-LICENSE.txt");
        assert!(l.contains("IPA") && l.to_lowercase().contains("license"));
    }
}
