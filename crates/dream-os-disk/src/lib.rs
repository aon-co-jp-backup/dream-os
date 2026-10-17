//! dream-os-disk: HDDのフォーマット、Windows/Linux/DreamOSの同居レイアウト、
//! USB/CDインストーラー媒体の生成。外部クレート依存なし。

pub mod bootcfg;
pub mod fat32;
pub mod gpt;
pub mod iso9660;
pub mod layout;
pub mod media;

/// 日時(UTC)。FAT/ISO9660のタイムスタンプ用。
#[derive(Clone, Copy, Debug)]
pub struct Stamp {
    pub y: u16,
    pub mo: u8,
    pub d: u8,
    pub h: u8,
    pub mi: u8,
    pub s: u8,
}

impl Stamp {
    pub fn fixed() -> Stamp {
        Stamp { y: 2026, mo: 10, d: 18, h: 0, mi: 0, s: 0 }
    }

    pub fn now() -> Stamp {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let days = secs.div_euclid(86_400);
        let rem = secs.rem_euclid(86_400);
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
        Stamp {
            y: y as u16,
            mo: m as u8,
            d: d as u8,
            h: (rem / 3600) as u8,
            mi: (rem % 3600 / 60) as u8,
            s: (rem % 60) as u8,
        }
    }

    /// FATの(日付, 時刻)
    pub fn dos(&self) -> (u16, u16) {
        let date = ((self.y.max(1980) - 1980) << 9) | ((self.mo as u16) << 5) | self.d as u16;
        let time = ((self.h as u16) << 11) | ((self.mi as u16) << 5) | (self.s as u16 / 2);
        (date, time)
    }

    /// ISO9660ディレクトリレコードの7バイト日時
    pub fn iso7(&self) -> [u8; 7] {
        [(self.y.saturating_sub(1900)) as u8, self.mo, self.d, self.h, self.mi, self.s, 0]
    }

    /// ISO9660ボリューム記述子の17文字日時
    pub fn iso17(&self) -> [u8; 17] {
        let s = format!("{:04}{:02}{:02}{:02}{:02}{:02}00", self.y, self.mo, self.d, self.h, self.mi, self.s);
        let mut b = [0u8; 17];
        b[..16].copy_from_slice(s.as_bytes());
        b
    }
}
