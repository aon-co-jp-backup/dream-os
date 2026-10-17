//! GPT(GUIDパーティションテーブル)とプロテクティブMBRの生成・検証。512バイトセクタ前提。

use std::io::{Seek, SeekFrom, Write};

pub const SECTOR: u64 = 512;
const ENTRIES: usize = 128;
const ENTRY_SIZE: usize = 128;
const ENTRY_SECTORS: u64 = (ENTRIES * ENTRY_SIZE) as u64 / SECTOR; // 32

pub const TYPE_ESP: &str = "C12A7328-F81F-11D2-BA4B-00A0C93EC93B";
pub const TYPE_MS_BASIC: &str = "EBD0A0A2-B9E5-4433-87C0-68B6B72699C7";
pub const TYPE_MS_RESERVED: &str = "E3C9E316-0B5C-4DB8-817D-F92DF00215AE";
pub const TYPE_LINUX_FS: &str = "0FC63DAF-8483-4772-8E79-3D69D8477DE4";
pub const TYPE_LINUX_SWAP: &str = "0657FD6D-A4AB-43C4-84E5-0933C84B4F4F";
/// DreamOS専用に定義した型GUID(既存の登録済みGUIDとは無関係の任意値)。
pub const TYPE_DREAMOS: &str = "D5EA3051-0A60-4D5E-9D2A-5E41AA00D0EA";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Partition {
    pub type_guid: [u8; 16],
    pub unique: [u8; 16],
    pub first_lba: u64,
    pub last_lba: u64,
    pub attrs: u64,
    pub name: String,
}

pub fn guid_bytes(s: &str) -> Result<[u8; 16], String> {
    let h: String = s.chars().filter(|c| *c != '-').collect();
    if h.len() != 32 {
        return Err(format!("GUIDの長さが不正: {s}"));
    }
    let mut b = [0u8; 16];
    for i in 0..16 {
        b[i] = u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).map_err(|e| e.to_string())?;
    }
    // 先頭3フィールドはリトルエンディアンで格納される
    b[0..4].reverse();
    b[4..6].reverse();
    b[6..8].reverse();
    Ok(b)
}

/// 決定的な擬似乱数のGUID(version 4形式)。`seed`と`n`から作る。
pub fn unique_guid(seed: u64, n: u64) -> [u8; 16] {
    let mut x = seed ^ n.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1EA_5000_0000_0001;
    let mut out = [0u8; 16];
    for chunk in out.chunks_mut(8) {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
        chunk.copy_from_slice(&x.to_le_bytes());
    }
    out[7] = (out[7] & 0x0F) | 0x40;
    out[8] = (out[8] & 0x3F) | 0x80;
    out
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn entries_blob(parts: &[Partition]) -> Vec<u8> {
    let mut v = vec![0u8; ENTRIES * ENTRY_SIZE];
    for (i, p) in parts.iter().enumerate() {
        let e = &mut v[i * ENTRY_SIZE..(i + 1) * ENTRY_SIZE];
        e[0..16].copy_from_slice(&p.type_guid);
        e[16..32].copy_from_slice(&p.unique);
        e[32..40].copy_from_slice(&p.first_lba.to_le_bytes());
        e[40..48].copy_from_slice(&p.last_lba.to_le_bytes());
        e[48..56].copy_from_slice(&p.attrs.to_le_bytes());
        for (j, u) in p.name.encode_utf16().take(36).enumerate() {
            e[56 + j * 2..58 + j * 2].copy_from_slice(&u.to_le_bytes());
        }
    }
    v
}

fn header(total: u64, backup: bool, disk: [u8; 16], entries_crc: u32) -> [u8; 512] {
    let mut h = [0u8; 512];
    h[0..8].copy_from_slice(b"EFI PART");
    h[8..12].copy_from_slice(&0x0001_0000u32.to_le_bytes());
    h[12..16].copy_from_slice(&92u32.to_le_bytes());
    let (cur, oth, ent) = if backup {
        (total - 1, 1, total - 1 - ENTRY_SECTORS)
    } else {
        (1, total - 1, 2)
    };
    h[24..32].copy_from_slice(&cur.to_le_bytes());
    h[32..40].copy_from_slice(&oth.to_le_bytes());
    h[40..48].copy_from_slice(&(2 + ENTRY_SECTORS).to_le_bytes()); // first usable = 34
    h[48..56].copy_from_slice(&(total - 2 - ENTRY_SECTORS).to_le_bytes()); // last usable
    h[56..72].copy_from_slice(&disk);
    h[72..80].copy_from_slice(&ent.to_le_bytes());
    h[80..84].copy_from_slice(&(ENTRIES as u32).to_le_bytes());
    h[84..88].copy_from_slice(&(ENTRY_SIZE as u32).to_le_bytes());
    h[88..92].copy_from_slice(&entries_crc.to_le_bytes());
    let c = crc32(&h[0..92]);
    h[16..20].copy_from_slice(&c.to_le_bytes());
    h
}

fn protective_mbr(total: u64) -> [u8; 512] {
    let mut m = [0u8; 512];
    m[446] = 0x00;
    m[447..450].copy_from_slice(&[0x00, 0x02, 0x00]);
    m[450] = 0xEE;
    m[451..454].copy_from_slice(&[0xFF, 0xFF, 0xFF]);
    m[454..458].copy_from_slice(&1u32.to_le_bytes());
    let sz = (total - 1).min(0xFFFF_FFFF) as u32;
    m[458..462].copy_from_slice(&sz.to_le_bytes());
    m[510] = 0x55;
    m[511] = 0xAA;
    m
}

pub struct GptImage {
    /// LBA0(MBR)〜LBA33(エントリ配列)。先頭から書く。
    pub primary: Vec<u8>,
    /// 末尾33セクタ(エントリ配列32 + バックアップヘッダ)。LBA `total-33`から書く。
    pub backup: Vec<u8>,
}

pub fn build(total: u64, disk: [u8; 16], parts: &[Partition]) -> Result<GptImage, String> {
    if total < 2 + ENTRY_SECTORS * 2 + 2 {
        return Err("ディスクが小さすぎます".into());
    }
    if parts.len() > ENTRIES {
        return Err("パーティション数が128を超えています".into());
    }
    let first_usable = 2 + ENTRY_SECTORS;
    let last_usable = total - 2 - ENTRY_SECTORS;
    let mut sorted: Vec<&Partition> = parts.iter().collect();
    sorted.sort_by_key(|p| p.first_lba);
    for (i, p) in sorted.iter().enumerate() {
        if p.first_lba < first_usable || p.last_lba > last_usable || p.first_lba > p.last_lba {
            return Err(format!("パーティション'{}'が使用可能範囲外です", p.name));
        }
        if i > 0 && sorted[i - 1].last_lba >= p.first_lba {
            return Err(format!("パーティション'{}'が重なっています", p.name));
        }
    }
    let blob = entries_blob(parts);
    let ecrc = crc32(&blob);
    let mut primary = Vec::with_capacity(34 * 512);
    primary.extend_from_slice(&protective_mbr(total));
    primary.extend_from_slice(&header(total, false, disk, ecrc));
    primary.extend_from_slice(&blob);
    let mut backup = Vec::with_capacity(33 * 512);
    backup.extend_from_slice(&blob);
    backup.extend_from_slice(&header(total, true, disk, ecrc));
    Ok(GptImage { primary, backup })
}

pub fn write<W: Write + Seek>(w: &mut W, total: u64, img: &GptImage) -> std::io::Result<()> {
    w.seek(SeekFrom::Start(0))?;
    w.write_all(&img.primary)?;
    w.seek(SeekFrom::Start((total - 1 - ENTRY_SECTORS) * SECTOR))?;
    w.write_all(&img.backup)?;
    w.flush()
}

/// 先頭34セクタから読み戻し、CRCを検証してパーティション一覧を返す。
pub fn parse(primary: &[u8]) -> Result<Vec<Partition>, String> {
    if primary.len() < 34 * 512 || &primary[512..520] != b"EFI PART" {
        return Err("GPTヘッダがありません".into());
    }
    if primary[510] != 0x55 || primary[511] != 0xAA || primary[450] != 0xEE {
        return Err("プロテクティブMBRが不正です".into());
    }
    let mut h = primary[512..604].to_vec();
    let stored = u32::from_le_bytes(h[16..20].try_into().unwrap());
    h[16..20].fill(0);
    if crc32(&h) != stored {
        return Err("ヘッダCRC不一致".into());
    }
    let blob = &primary[1024..1024 + ENTRIES * ENTRY_SIZE];
    let ecrc = u32::from_le_bytes(primary[512 + 88..512 + 92].try_into().unwrap());
    if crc32(blob) != ecrc {
        return Err("エントリ配列CRC不一致".into());
    }
    let mut out = Vec::new();
    for e in blob.chunks(ENTRY_SIZE) {
        if e[0..16].iter().all(|b| *b == 0) {
            continue;
        }
        let u16s: Vec<u16> = e[56..128]
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|c| *c != 0)
            .collect();
        out.push(Partition {
            type_guid: e[0..16].try_into().unwrap(),
            unique: e[16..32].try_into().unwrap(),
            first_lba: u64::from_le_bytes(e[32..40].try_into().unwrap()),
            last_lba: u64::from_le_bytes(e[40..48].try_into().unwrap()),
            attrs: u64::from_le_bytes(e[48..56].try_into().unwrap()),
            name: String::from_utf16_lossy(&u16s),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_known_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn esp_guid_is_mixed_endian() {
        let g = guid_bytes(TYPE_ESP).unwrap();
        assert_eq!(&g[0..4], &[0x28, 0x73, 0x2A, 0xC1]);
    }

    #[test]
    fn roundtrip_and_overlap_rejected() {
        let total = 200_000;
        let p = |a, b, n: &str| Partition {
            type_guid: guid_bytes(TYPE_LINUX_FS).unwrap(),
            unique: unique_guid(1, a),
            first_lba: a,
            last_lba: b,
            attrs: 0,
            name: n.into(),
        };
        let ok = build(total, unique_guid(1, 0), &[p(2048, 4095, "日本語"), p(4096, 9999, "b")]).unwrap();
        let back = parse(&ok.primary).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].name, "日本語");
        assert!(build(total, unique_guid(1, 0), &[p(2048, 5000, "a"), p(4096, 9999, "b")]).is_err());
        assert!(build(total, unique_guid(1, 0), &[p(10, 4095, "a")]).is_err());
    }
}
