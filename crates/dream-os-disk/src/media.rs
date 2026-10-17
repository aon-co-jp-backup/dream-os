//! 高レベル操作: ディスクへのパーティション作成+フォーマット、インストーラーISO(CD/USB用)の作成。

use crate::bootcfg;
use crate::fat32::Fat32;
use crate::gpt;
use crate::iso9660::{self, IsoOptions, Node};
use crate::layout::{self, Fs, Options, Os, Planned, Want};
use crate::Stamp;
use std::io::{Seek, SeekFrom, Write};

/// メモリ上で作るFAT32の上限。これより大きい領域は作成だけして、フォーマットはOS側に任せる。
pub const MAX_INMEMORY_FAT: u64 = 2 * 1024 * 1024 * 1024;

pub struct DiskSpec<'a> {
    pub total_sectors: u64,
    pub wants: &'a [Want],
    pub opts: &'a Options,
    pub default_os: Os,
    pub seed: u64,
    /// ESPの`EFI/BOOT/BOOTX64.EFI`に置くブートローダー(GRUBのスタンドアロンEFIなど)
    pub bootloader: Option<&'a [u8]>,
    /// 書き込み先がファイルイメージなら、全ゼロのチャンクを書かずにスパースのままにする
    pub sparse: bool,
}

#[derive(Debug)]
pub struct Report {
    pub plan: Vec<Planned>,
    pub formatted: Vec<String>,
    pub left_to_os: Vec<String>,
}

fn write_region<W: Write + Seek>(w: &mut W, byte_off: u64, data: &[u8], sparse: bool) -> std::io::Result<()> {
    const CHUNK: usize = 1024 * 1024;
    let mut off = 0usize;
    while off < data.len() {
        let end = (off + CHUNK).min(data.len());
        let c = &data[off..end];
        if !(sparse && c.iter().all(|b| *b == 0)) {
            w.seek(SeekFrom::Start(byte_off + off as u64))?;
            w.write_all(c)?;
        }
        off = end;
    }
    Ok(())
}

/// GPTを書き、ESPと小さなFAT32(共有領域)をフォーマットする。NTFS/ext4/swapは領域のみ作る
/// (WindowsインストーラーやLinuxインストーラーが自分でフォーマットするのが最も安全なため)。
pub fn format_disk<W: Write + Seek>(w: &mut W, spec: &DiskSpec) -> Result<Report, String> {
    let plan = layout::plan(spec.total_sectors, spec.wants, spec.opts)?;
    let parts = layout::to_gpt(&plan, spec.seed)?;
    let img = gpt::build(spec.total_sectors, gpt::unique_guid(spec.seed, 0), &parts)?;
    let io = |e: std::io::Error| e.to_string();
    gpt::write(w, spec.total_sectors, &img).map_err(io)?;

    let oses: Vec<Os> = spec.wants.iter().map(|x| x.os).collect();
    let stamp = Stamp::now();
    let mut formatted = Vec::new();
    let mut left = Vec::new();
    for (i, p) in plan.iter().enumerate() {
        if p.fs != Fs::Fat32 {
            if p.fs != Fs::None {
                left.push(format!("#{} {} ({:?}): OSのインストーラーでフォーマット", i + 1, p.name, p.fs));
            }
            continue;
        }
        if p.sectors() * 512 > MAX_INMEMORY_FAT {
            left.push(format!("#{} {} (FAT32 {} MiB): 大きいためOS側でフォーマット", i + 1, p.name, p.sectors() * 512 >> 20));
            continue;
        }
        let label = if p.type_guid == gpt::TYPE_ESP { "ESP" } else { "SHARED" };
        let mut fat = Fat32::format(p.sectors(), p.first_lba as u32, label, 0xD5EA_0000 + i as u32, stamp)?;
        if p.type_guid == gpt::TYPE_ESP {
            fat.write_file("EFI/BOOT/grub.cfg", bootcfg::grub_cfg(&oses, spec.default_os, 5).as_bytes())?;
            if let Some(b) = spec.bootloader {
                fat.write_file("EFI/BOOT/BOOTX64.EFI", b)?;
            }
        }
        write_region(w, p.first_lba * 512, &fat.finish(), spec.sparse).map_err(io)?;
        formatted.push(format!("#{} {} (FAT32)", i + 1, p.name));
    }
    w.flush().map_err(io)?;
    Ok(Report { plan, formatted, left_to_os: left })
}

/// ブート可能なインストーラーISOを作る。`payload`は媒体のルートに置くファイル群、
/// `oses`/`default_os`はESP内ブートメニュー、`bootloader`はUEFIブートローダー(任意)。
pub fn build_installer_iso(
    payload: Node,
    oses: &[Os],
    default_os: Os,
    bootloader: Option<&[u8]>,
    volume_id: &str,
) -> Result<Vec<u8>, String> {
    let stamp = Stamp::now();
    // UEFI用ESPイメージ(48MiB。FAT32の最小クラスタ数を満たす小さめのサイズ)
    let mut esp = Fat32::format(48 * 1024 * 2, 0, "DREAMOSCD", 0xD5EA_C0DE, stamp)?;
    esp.write_file("EFI/BOOT/grub.cfg", bootcfg::grub_cfg(oses, default_os, 10).as_bytes())?;
    if let Some(b) = bootloader {
        esp.write_file("EFI/BOOT/BOOTX64.EFI", b)?;
    }
    let esp_img = esp.finish();
    let mut root = payload;
    // ファイルツリー側にも置く: USBメモリへ複写したときにもUEFIで起動できるように
    root.add_file("EFI/BOOT/grub.cfg", bootcfg::grub_cfg(oses, default_os, 10).as_bytes().to_vec());
    if let Some(b) = bootloader {
        root.add_file("EFI/BOOT/BOOTX64.EFI", b.to_vec());
    }
    iso9660::build(&root, &IsoOptions { volume_id: volume_id.into(), efi_boot_image: Some(esp_img), stamp })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn format_disk_image_in_memory_is_readable() {
        let total = 6u64 * 1024 * 1024 * 1024 / 512;
        let mut disk = Cursor::new(Vec::<u8>::new());
        disk.get_mut().resize(1, 0);
        let wants = [
            Want { os: Os::Windows, size_mib: Some(2048) },
            Want { os: Os::Linux, size_mib: None },
        ];
        // 6GiBのVecは重いので、先頭34セクタ+ESP領域だけを持つ疎なライターで受ける
        struct Sparse(std::collections::BTreeMap<u64, Vec<u8>>, u64);
        impl Write for Sparse {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.insert(self.1, b.to_vec());
                self.1 += b.len() as u64;
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        impl Seek for Sparse {
            fn seek(&mut self, p: SeekFrom) -> std::io::Result<u64> {
                if let SeekFrom::Start(o) = p {
                    self.1 = o;
                }
                Ok(self.1)
            }
        }
        let _ = &mut disk;
        let mut w = Sparse(Default::default(), 0);
        let opts = Options { shared_mib: Some(256), ..Default::default() };
        let spec = DiskSpec { total_sectors: total, wants: &wants, opts: &opts, default_os: Os::Linux, seed: 3, bootloader: Some(b"EFI"), sparse: true };
        let r = format_disk(&mut w, &spec).unwrap();
        assert!(r.formatted.iter().any(|s| s.contains("EFI System")));
        assert!(r.left_to_os.iter().any(|s| s.contains("Windows")));
        let head = w.0.get(&0).unwrap();
        let parts = gpt::parse(head).unwrap();
        assert_eq!(parts.len(), r.plan.len());
        assert_eq!(parts[0].name, "EFI System");
    }

    #[test]
    fn installer_iso_has_payload_and_boot_menu() {
        let mut p = Node::new_dir();
        p.add_file("dream-os-installer.exe", vec![9; 3000]);
        let iso = build_installer_iso(p, &[Os::Windows, Os::Linux], Os::Windows, Some(b"EFI"), "DREAMOS").unwrap();
        assert_eq!(iso9660::read_file(&iso, "dream-os-installer.exe", true).unwrap().len(), 3000);
        assert!(iso9660::read_file(&iso, "EFI/BOOT/grub.cfg", true).unwrap().starts_with(b"# DreamOS"));
    }
}
