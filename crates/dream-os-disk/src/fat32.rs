//! FAT32のフォーマッタと、小さなファイル群を書き込む最小実装(LFN=長いファイル名対応)。
//! ESP(EFIシステムパーティション)や共有データ領域、UEFIブート用イメージの作成に使う。

use crate::Stamp;
use std::collections::{HashMap, HashSet};

const BPS: usize = 512;
const RESERVED: u32 = 32;
const EOC: u32 = 0x0FFF_FFFF;

struct DirState {
    chain: Vec<u32>,
    used: usize,
}

pub struct Fat32 {
    buf: Vec<u8>,
    spc: u32,
    fat_sectors: u32,
    clusters: u32,
    next_free: u32,
    stamp: Stamp,
    dirs: HashMap<u32, DirState>,
    paths: HashMap<String, u32>,
    short_used: HashMap<u32, HashSet<[u8; 11]>>,
}

fn spc_for(sectors: u64) -> u32 {
    match sectors {
        0..=532_480 => 1,
        532_481..=16_777_216 => 8,
        16_777_217..=33_554_432 => 16,
        33_554_433..=67_108_864 => 32,
        _ => 64,
    }
}

fn checksum(short: &[u8; 11]) -> u8 {
    short.iter().fold(0u8, |s, &c| (s >> 1).wrapping_add((s & 1) << 7).wrapping_add(c))
}

fn valid_short_char(c: char) -> bool {
    c.is_ascii_uppercase() || c.is_ascii_digit() || "$%'-_@~`!(){}^#&".contains(c)
}

impl Fat32 {
    /// `total_sectors`セクタ(512B)のFAT32を作る。`hidden`はパーティション先頭のLBA。
    pub fn format(total_sectors: u64, hidden: u32, label: &str, vol_id: u32, stamp: Stamp) -> Result<Fat32, String> {
        let spc = spc_for(total_sectors);
        let est_clusters = total_sectors / spc as u64;
        let fat_sectors = (((est_clusters + 2) * 4).div_ceil(BPS as u64)) as u32;
        let meta = RESERVED as u64 + 2 * fat_sectors as u64;
        if total_sectors <= meta {
            return Err("FAT32には小さすぎます".into());
        }
        let clusters = ((total_sectors - meta) / spc as u64) as u32;
        if clusters < 65_525 {
            return Err(format!(
                "FAT32には小さすぎます(クラスタ数{clusters} < 65525、約34MiB以上が必要)"
            ));
        }
        let mut f = Fat32 {
            buf: vec![0u8; (total_sectors as usize) * BPS],
            spc,
            fat_sectors,
            clusters,
            next_free: 3,
            stamp,
            dirs: HashMap::new(),
            paths: HashMap::new(),
            short_used: HashMap::new(),
        };
        let bs = f.boot_sector(total_sectors as u32, hidden, label, vol_id);
        f.buf[0..BPS].copy_from_slice(&bs);
        f.buf[6 * BPS..7 * BPS].copy_from_slice(&bs);
        f.set_fat(0, 0x0FFF_FFF8);
        f.set_fat(1, EOC);
        f.set_fat(2, EOC);
        f.dirs.insert(2, DirState { chain: vec![2], used: 0 });
        f.paths.insert(String::new(), 2);
        let mut lab = [b' '; 11];
        for (i, c) in label.bytes().take(11).enumerate() {
            lab[i] = c.to_ascii_uppercase();
        }
        let e = f.short_entry(lab, 0x08, 0, 0);
        f.push_entries(2, &[e])?;
        Ok(f)
    }

    fn boot_sector(&self, total: u32, hidden: u32, label: &str, vol_id: u32) -> [u8; BPS] {
        let mut b = [0u8; BPS];
        b[0..3].copy_from_slice(&[0xEB, 0x58, 0x90]);
        b[3..11].copy_from_slice(b"DREAMOS ");
        b[11..13].copy_from_slice(&(BPS as u16).to_le_bytes());
        b[13] = self.spc as u8;
        b[14..16].copy_from_slice(&(RESERVED as u16).to_le_bytes());
        b[16] = 2;
        b[21] = 0xF8;
        b[24..26].copy_from_slice(&63u16.to_le_bytes());
        b[26..28].copy_from_slice(&255u16.to_le_bytes());
        b[28..32].copy_from_slice(&hidden.to_le_bytes());
        b[32..36].copy_from_slice(&total.to_le_bytes());
        b[36..40].copy_from_slice(&self.fat_sectors.to_le_bytes());
        b[44..48].copy_from_slice(&2u32.to_le_bytes());
        b[48..50].copy_from_slice(&1u16.to_le_bytes());
        b[50..52].copy_from_slice(&6u16.to_le_bytes());
        b[64] = 0x80;
        b[66] = 0x29;
        b[67..71].copy_from_slice(&vol_id.to_le_bytes());
        let mut lab = [b' '; 11];
        for (i, c) in label.bytes().take(11).enumerate() {
            lab[i] = c.to_ascii_uppercase();
        }
        b[71..82].copy_from_slice(&lab);
        b[82..90].copy_from_slice(b"FAT32   ");
        b[510] = 0x55;
        b[511] = 0xAA;
        b
    }

    fn fat_off(&self, n: u32, copy: u32) -> usize {
        (RESERVED + copy * self.fat_sectors) as usize * BPS + n as usize * 4
    }

    fn set_fat(&mut self, n: u32, v: u32) {
        for c in 0..2 {
            let o = self.fat_off(n, c);
            self.buf[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
    }

    fn get_fat(&self, n: u32) -> u32 {
        let o = self.fat_off(n, 0);
        u32::from_le_bytes(self.buf[o..o + 4].try_into().unwrap()) & 0x0FFF_FFFF
    }

    fn cluster_bytes(&self) -> usize {
        self.spc as usize * BPS
    }

    fn cluster_off(&self, n: u32) -> usize {
        let data_start = RESERVED as usize + 2 * self.fat_sectors as usize;
        (data_start + (n as usize - 2) * self.spc as usize) * BPS
    }

    fn alloc(&mut self, prev: Option<u32>) -> Result<u32, String> {
        let n = self.next_free;
        if n >= self.clusters + 2 {
            return Err("FAT32の空きがありません".into());
        }
        self.next_free += 1;
        self.set_fat(n, EOC);
        if let Some(p) = prev {
            self.set_fat(p, n);
        }
        Ok(n)
    }

    fn short_entry(&self, name: [u8; 11], attr: u8, cluster: u32, size: u32) -> [u8; 32] {
        let (d, t) = self.stamp.dos();
        let mut e = [0u8; 32];
        e[0..11].copy_from_slice(&name);
        e[11] = attr;
        e[14..16].copy_from_slice(&t.to_le_bytes());
        e[16..18].copy_from_slice(&d.to_le_bytes());
        e[18..20].copy_from_slice(&d.to_le_bytes());
        e[20..22].copy_from_slice(&((cluster >> 16) as u16).to_le_bytes());
        e[22..24].copy_from_slice(&t.to_le_bytes());
        e[24..26].copy_from_slice(&d.to_le_bytes());
        e[26..28].copy_from_slice(&(cluster as u16).to_le_bytes());
        e[28..32].copy_from_slice(&size.to_le_bytes());
        e
    }

    fn push_entries(&mut self, dir: u32, entries: &[[u8; 32]]) -> Result<(), String> {
        let epc = self.cluster_bytes() / 32;
        for e in entries {
            let (slot, last) = {
                let st = self.dirs.get(&dir).ok_or("ディレクトリ不明")?;
                (st.used, *st.chain.last().unwrap())
            };
            if slot / epc >= self.dirs[&dir].chain.len() {
                let n = self.alloc(Some(last))?;
                self.dirs.get_mut(&dir).unwrap().chain.push(n);
            }
            let cl = self.dirs[&dir].chain[slot / epc];
            let off = self.cluster_off(cl) + (slot % epc) * 32;
            self.buf[off..off + 32].copy_from_slice(e);
            self.dirs.get_mut(&dir).unwrap().used += 1;
        }
        Ok(())
    }

    fn make_names(&mut self, dir: u32, name: &str) -> Result<([u8; 11], Option<Vec<u16>>), String> {
        if name.is_empty() || name.contains(['\\', ':', '*', '?', '"', '<', '>', '|']) {
            return Err(format!("使えないファイル名: {name}"));
        }
        let used = self.short_used.entry(dir).or_default();
        let (base, ext) = match name.rfind('.') {
            Some(i) if i > 0 => (&name[..i], &name[i + 1..]),
            _ => (name, ""),
        };
        let fits = base.chars().count() <= 8 && ext.chars().count() <= 3 && !ext.contains('.')
            && !base.contains('.') && base.chars().chain(ext.chars()).all(valid_short_char);
        let mut short = [b' '; 11];
        if fits {
            for (i, c) in base.bytes().enumerate() {
                short[i] = c;
            }
            for (i, c) in ext.bytes().enumerate() {
                short[8 + i] = c;
            }
            if !used.insert(short) {
                return Err(format!("同名のファイルがあります: {name}"));
            }
            return Ok((short, None));
        }
        let clean = |s: &str, n: usize| -> Vec<u8> {
            s.chars()
                .filter(|c| *c != ' ' && *c != '.')
                .map(|c| {
                    let u = c.to_ascii_uppercase();
                    if valid_short_char(u) { u as u8 } else { b'_' }
                })
                .take(n)
                .collect()
        };
        let b6 = clean(base, 6);
        let e3 = clean(ext, 3);
        for n in 1..1_000_000u32 {
            let tail = format!("~{n}");
            let mut s = [b' '; 11];
            let keep = 8usize.saturating_sub(tail.len()).min(b6.len());
            s[..keep].copy_from_slice(&b6[..keep]);
            s[keep..keep + tail.len()].copy_from_slice(tail.as_bytes());
            s[8..8 + e3.len()].copy_from_slice(&e3);
            if used.insert(s) {
                return Ok((s, Some(name.encode_utf16().collect())));
            }
        }
        Err("短い名前を作れません".into())
    }

    fn add_dir_entry(&mut self, dir: u32, name: &str, attr: u8, cluster: u32, size: u32) -> Result<(), String> {
        let (short, lfn) = self.make_names(dir, name)?;
        let mut entries: Vec<[u8; 32]> = Vec::new();
        if let Some(u) = lfn {
            let sum = checksum(&short);
            let n = u.len().div_ceil(13);
            for k in (0..n).rev() {
                let mut e = [0u8; 32];
                e[0] = (k as u8 + 1) | if k == n - 1 { 0x40 } else { 0 };
                e[11] = 0x0F;
                e[13] = sum;
                let offs = [1usize, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30];
                for (j, o) in offs.iter().enumerate() {
                    let pos = k * 13 + j;
                    let c: u16 = if pos < u.len() { u[pos] } else if pos == u.len() { 0 } else { 0xFFFF };
                    e[*o..*o + 2].copy_from_slice(&c.to_le_bytes());
                }
                entries.push(e);
            }
        }
        entries.push(self.short_entry(short, attr, cluster, size));
        self.push_entries(dir, &entries)
    }

    fn ensure_dir(&mut self, path: &str) -> Result<u32, String> {
        let key = path.trim_matches('/').to_string();
        if let Some(c) = self.paths.get(&key) {
            return Ok(*c);
        }
        let (parent_path, name) = match key.rfind('/') {
            Some(i) => (key[..i].to_string(), key[i + 1..].to_string()),
            None => (String::new(), key.clone()),
        };
        let parent = self.ensure_dir(&parent_path)?;
        let c = self.alloc(None)?;
        self.dirs.insert(c, DirState { chain: vec![c], used: 0 });
        let mut dot = [b' '; 11];
        dot[0] = b'.';
        let mut dotdot = [b' '; 11];
        dotdot[0] = b'.';
        dotdot[1] = b'.';
        let e1 = self.short_entry(dot, 0x10, c, 0);
        let e2 = self.short_entry(dotdot, 0x10, if parent == 2 { 0 } else { parent }, 0);
        self.push_entries(c, &[e1, e2])?;
        self.add_dir_entry(parent, &name, 0x10, c, 0)?;
        self.paths.insert(key, c);
        Ok(c)
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), String> {
        self.ensure_dir(path).map(|_| ())
    }

    pub fn write_file(&mut self, path: &str, data: &[u8]) -> Result<(), String> {
        let p = path.trim_matches('/');
        let (dir_path, name) = match p.rfind('/') {
            Some(i) => (&p[..i], &p[i + 1..]),
            None => ("", p),
        };
        let dir = self.ensure_dir(dir_path)?;
        let cs = self.cluster_bytes();
        let mut first = 0u32;
        let mut prev: Option<u32> = None;
        for chunk in data.chunks(cs) {
            let n = self.alloc(prev)?;
            if first == 0 {
                first = n;
            }
            let o = self.cluster_off(n);
            self.buf[o..o + chunk.len()].copy_from_slice(chunk);
            prev = Some(n);
        }
        self.add_dir_entry(dir, name, 0x20, first, data.len() as u32)
    }

    /// FSInfoを確定してイメージ全体を返す。
    pub fn finish(mut self) -> Vec<u8> {
        let mut fs = [0u8; BPS];
        fs[0..4].copy_from_slice(&0x4161_5252u32.to_le_bytes());
        fs[484..488].copy_from_slice(&0x6141_7272u32.to_le_bytes());
        fs[488..492].copy_from_slice(&(self.clusters + 2 - self.next_free).to_le_bytes());
        fs[492..496].copy_from_slice(&self.next_free.to_le_bytes());
        fs[508..512].copy_from_slice(&0xAA55_0000u32.to_le_bytes());
        self.buf[BPS..2 * BPS].copy_from_slice(&fs);
        self.buf[7 * BPS..8 * BPS].copy_from_slice(&fs);
        self.buf
    }

    /// 検証用: パス(大文字小文字を区別しない)のファイル内容を読む。
    pub fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        let mut cluster = 2u32;
        let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
        for (i, part) in parts.iter().enumerate() {
            let (first, size, is_dir) = self.lookup(cluster, part)?;
            if i + 1 == parts.len() {
                return if is_dir { None } else { Some(self.read_chain(first, size as usize)) };
            }
            if !is_dir {
                return None;
            }
            cluster = first;
        }
        None
    }

    fn read_chain(&self, first: u32, size: usize) -> Vec<u8> {
        let mut out = Vec::new();
        let mut c = first;
        while c >= 2 && c < 0x0FFF_FFF8 && out.len() < size {
            let o = self.cluster_off(c);
            out.extend_from_slice(&self.buf[o..o + self.cluster_bytes()]);
            c = self.get_fat(c);
        }
        out.truncate(size);
        out
    }

    fn lookup(&self, dir: u32, want: &str) -> Option<(u32, u32, bool)> {
        let raw = self.read_chain(dir, usize::MAX >> 1);
        let mut lfn: Vec<(u8, Vec<u16>)> = Vec::new();
        for e in raw.chunks(32) {
            if e[0] == 0 {
                break;
            }
            if e[0] == 0xE5 {
                lfn.clear();
                continue;
            }
            if e[11] == 0x0F {
                let mut u = Vec::new();
                for o in [1usize, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30] {
                    u.push(u16::from_le_bytes([e[o], e[o + 1]]));
                }
                lfn.push((e[0] & 0x1F, u));
                continue;
            }
            if e[11] & 0x08 != 0 {
                lfn.clear();
                continue;
            }
            let name = if lfn.is_empty() {
                let b = String::from_utf8_lossy(&e[0..8]).trim_end().to_string();
                let x = String::from_utf8_lossy(&e[8..11]).trim_end().to_string();
                if x.is_empty() { b } else { format!("{b}.{x}") }
            } else {
                lfn.sort_by_key(|l| l.0);
                let all: Vec<u16> = lfn.iter().flat_map(|l| l.1.clone()).take_while(|c| *c != 0 && *c != 0xFFFF).collect();
                String::from_utf16_lossy(&all)
            };
            lfn.clear();
            if name.eq_ignore_ascii_case(want) {
                let cl = ((u16::from_le_bytes([e[20], e[21]]) as u32) << 16) | u16::from_le_bytes([e[26], e[27]]) as u32;
                return Some((cl, u32::from_le_bytes(e[28..32].try_into().unwrap()), e[11] & 0x10 != 0));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> Fat32 {
        Fat32::format(80_000, 2048, "DREAMOS", 0x1234_5678, Stamp::fixed()).unwrap()
    }

    #[test]
    fn too_small_rejected() {
        assert!(Fat32::format(10_000, 0, "X", 1, Stamp::fixed()).is_err());
    }

    #[test]
    fn files_roundtrip_with_long_names_and_subdirs() {
        let mut f = fresh();
        let big: Vec<u8> = (0..20_000u32).map(|i| (i % 251) as u8).collect();
        f.write_file("EFI/BOOT/BOOTX64.EFI", b"efi").unwrap();
        f.write_file("EFI/BOOT/grub.cfg", b"menu").unwrap();
        f.write_file("dream-os-installer.exe", &big).unwrap();
        f.write_file("日本語/読んでください.txt", b"utf16").unwrap();
        assert_eq!(f.read_file("EFI/BOOT/BOOTX64.EFI").unwrap(), b"efi");
        assert_eq!(f.read_file("efi/boot/GRUB.CFG").unwrap(), b"menu");
        assert_eq!(f.read_file("dream-os-installer.exe").unwrap(), big);
        assert_eq!(f.read_file("日本語/読んでください.txt").unwrap(), b"utf16");
        assert!(f.read_file("nothing").is_none());
    }

    #[test]
    fn many_entries_grow_directory_and_duplicates_rejected() {
        let mut f = fresh();
        for i in 0..400 {
            f.write_file(&format!("data/file-number-{i}.bin"), &[i as u8]).unwrap();
        }
        assert_eq!(f.read_file("data/file-number-399.bin").unwrap(), vec![399u32 as u8]);
        assert!(f.write_file("A.TXT", b"1").is_ok());
        assert!(f.write_file("A.TXT", b"2").is_err());
    }

    #[test]
    fn boot_sector_fields() {
        let img = fresh().finish();
        assert_eq!(&img[510..512], &[0x55, 0xAA]);
        assert_eq!(&img[82..90], b"FAT32   ");
        assert_eq!(&img[3..11], b"DREAMOS ");
        assert_eq!(&img[6 * 512..6 * 512 + 512], &img[0..512]);
    }
}
