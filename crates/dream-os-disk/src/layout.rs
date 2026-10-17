//! Windows / Linux / DreamOS を1台のディスクに同居させるパーティション配置の計算。

use crate::gpt::{self, Partition};

const MIB: u64 = 1024 * 1024;
const ALIGN: u64 = 2048; // 1MiB(512Bセクタ)

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Windows,
    Linux,
    DreamOs,
}

impl Os {
    pub fn parse(s: &str) -> Result<Os, String> {
        match s.to_ascii_lowercase().as_str() {
            "windows" | "win" => Ok(Os::Windows),
            "linux" => Ok(Os::Linux),
            "dreamos" | "dream-os" | "dream" => Ok(Os::DreamOs),
            _ => Err(format!("未知のOS: {s} (windows / linux / dreamos)")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fs {
    Fat32,
    Ntfs,
    Ext4,
    Swap,
    /// DreamOS独自(ファイルシステムは未確定、型GUIDのみ予約)
    DreamOs,
    /// 予約領域(MSR)
    None,
}

#[derive(Clone, Debug)]
pub struct Want {
    pub os: Os,
    /// MiB単位。`None`は「残り全部」(1つだけ指定可)。
    pub size_mib: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub esp_mib: u64,
    /// OS間で共有するFAT32データ領域(Windows/Linux/DreamOSから読み書きできる)
    pub shared_mib: Option<u64>,
    pub linux_swap_mib: Option<u64>,
}

impl Default for Options {
    fn default() -> Self {
        Options { esp_mib: 512, shared_mib: None, linux_swap_mib: None }
    }
}

#[derive(Clone, Debug)]
pub struct Planned {
    pub name: String,
    pub type_guid: &'static str,
    pub fs: Fs,
    pub first_lba: u64,
    pub last_lba: u64,
}

impl Planned {
    pub fn sectors(&self) -> u64 {
        self.last_lba - self.first_lba + 1
    }
}

fn align_up(x: u64) -> u64 {
    x.div_ceil(ALIGN) * ALIGN
}

pub fn plan(total_sectors: u64, wants: &[Want], opts: &Options) -> Result<Vec<Planned>, String> {
    if wants.is_empty() {
        return Err("OSを1つ以上指定してください".into());
    }
    if wants.iter().filter(|w| w.size_mib.is_none()).count() > 1 {
        return Err("サイズ省略(残り全部)は1つのOSだけに指定できます".into());
    }
    for (i, a) in wants.iter().enumerate() {
        if wants[..i].iter().any(|b| b.os == a.os) {
            return Err("同じOSを2回指定できません".into());
        }
    }
    let last_usable = total_sectors.checked_sub(34).ok_or("ディスクが小さすぎます")?;

    // (name, type, fs, size_mib(None=rest))
    let mut items: Vec<(String, &'static str, Fs, Option<u64>)> = Vec::new();
    items.push(("EFI System".into(), gpt::TYPE_ESP, Fs::Fat32, Some(opts.esp_mib)));
    if wants.iter().any(|w| w.os == Os::Windows) {
        items.push(("Microsoft reserved".into(), gpt::TYPE_MS_RESERVED, Fs::None, Some(16)));
    }
    for w in wants {
        match w.os {
            Os::Windows => items.push(("Windows".into(), gpt::TYPE_MS_BASIC, Fs::Ntfs, w.size_mib)),
            Os::Linux => {
                items.push(("Linux".into(), gpt::TYPE_LINUX_FS, Fs::Ext4, w.size_mib));
                if let Some(s) = opts.linux_swap_mib {
                    items.push(("Linux swap".into(), gpt::TYPE_LINUX_SWAP, Fs::Swap, Some(s)));
                }
            }
            Os::DreamOs => items.push(("DreamOS".into(), gpt::TYPE_DREAMOS, Fs::DreamOs, w.size_mib)),
        }
    }
    if let Some(s) = opts.shared_mib {
        items.push(("Shared".into(), gpt::TYPE_MS_BASIC, Fs::Fat32, Some(s)));
    }

    let fixed: u64 = items.iter().filter_map(|i| i.3).map(|m| align_up(m * MIB / 512)).sum();
    let start = ALIGN;
    let avail = (last_usable + 1).saturating_sub(start);
    if fixed > avail {
        return Err(format!(
            "ディスクに収まりません(必要 {} MiB > 空き {} MiB)",
            fixed * 512 / MIB,
            avail * 512 / MIB
        ));
    }
    let rest = avail - fixed;
    let has_rest = items.iter().any(|i| i.3.is_none());
    if has_rest && rest < ALIGN * 16 {
        return Err("「残り全部」に割り当てる空きがほとんどありません".into());
    }

    let mut out = Vec::new();
    let mut cur = start;
    for (name, ty, fs, size) in items {
        let sectors = match size {
            Some(m) => align_up(m * MIB / 512),
            None => rest / ALIGN * ALIGN,
        };
        out.push(Planned { name, type_guid: ty, fs, first_lba: cur, last_lba: cur + sectors - 1 });
        cur += sectors;
    }
    Ok(out)
}

pub fn to_gpt(plan: &[Planned], seed: u64) -> Result<Vec<Partition>, String> {
    plan.iter()
        .enumerate()
        .map(|(i, p)| {
            Ok(Partition {
                type_guid: gpt::guid_bytes(p.type_guid)?,
                unique: gpt::unique_guid(seed, i as u64 + 1),
                first_lba: p.first_lba,
                last_lba: p.last_lba,
                attrs: 0,
                name: p.name.clone(),
            })
        })
        .collect()
}

pub fn describe(plan: &[Planned]) -> String {
    let mut s = String::new();
    for (i, p) in plan.iter().enumerate() {
        s.push_str(&format!(
            "  #{:<2} {:<20} {:>10} MiB  LBA {}-{}  {:?}\n",
            i + 1,
            p.name,
            p.sectors() * 512 / MIB,
            p.first_lba,
            p.last_lba,
            p.fs
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_os_fit_and_do_not_overlap() {
        let total = 500u64 * 1024 * 1024 * 1024 / 512;
        let wants = [
            Want { os: Os::Windows, size_mib: Some(200 * 1024) },
            Want { os: Os::Linux, size_mib: Some(100 * 1024) },
            Want { os: Os::DreamOs, size_mib: None },
        ];
        let opts = Options { shared_mib: Some(20 * 1024), linux_swap_mib: Some(8 * 1024), ..Default::default() };
        let p = plan(total, &wants, &opts).unwrap();
        for w in p.windows(2) {
            assert!(w[0].last_lba < w[1].first_lba);
        }
        assert!(p.last().unwrap().last_lba <= total - 34);
        assert!(p.iter().all(|x| x.first_lba % 2048 == 0));
        let parts = to_gpt(&p, 7).unwrap();
        gpt::build(total, gpt::unique_guid(7, 0), &parts).unwrap();
    }

    #[test]
    fn too_big_and_duplicate_rejected() {
        let total = 10u64 * 1024 * 1024 * 1024 / 512;
        let big = [Want { os: Os::Windows, size_mib: Some(50 * 1024) }];
        assert!(plan(total, &big, &Options::default()).is_err());
        let dup = [Want { os: Os::Linux, size_mib: None }, Want { os: Os::Linux, size_mib: None }];
        assert!(plan(total, &dup, &Options::default()).is_err());
    }
}
