//! dream-os-disk コマンドライン。
//!
//!   dream-os-disk plan     --disk 500G --os windows=200G,linux=100G,dreamos [--shared 20G] [--swap 8G]
//!   dream-os-disk format   --image disk.img --size 8G --os ...            (イメージファイル)
//!   dream-os-disk format   --device PATH --size-bytes N --confirm "ERASE PATH N" --os ...  (実ディスク。全消去)
//!   dream-os-disk make-iso --payload DIR --out dreamos.iso --os ... [--bootloader BOOTX64.EFI]
//!   dream-os-disk grub-cfg --os ... [--default linux]
//!   dream-os-disk qemu     --host windows --guest linux --image linux.img

use dream_os_disk::iso9660::Node;
use dream_os_disk::layout::{self, Options, Os, Want};
use dream_os_disk::media::{self, DiskSpec};
use dream_os_disk::bootcfg;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

fn parse_size(s: &str) -> Result<u64, String> {
    let s = s.trim();
    let (num, mult) = match s.chars().last() {
        Some('G') | Some('g') => (&s[..s.len() - 1], 1024),
        Some('T') | Some('t') => (&s[..s.len() - 1], 1024 * 1024),
        Some('M') | Some('m') => (&s[..s.len() - 1], 1),
        _ => return Err(format!("サイズは 数字+M/G/T で指定: {s}")),
    };
    num.parse::<u64>().map(|n| n * mult).map_err(|e| format!("{s}: {e}"))
}

fn flags(args: &[String]) -> Result<HashMap<String, String>, String> {
    let mut m = HashMap::new();
    let mut i = 0;
    while i < args.len() {
        let k = args[i].strip_prefix("--").ok_or(format!("不明な引数: {}", args[i]))?;
        let v = args.get(i + 1).ok_or(format!("--{k} に値がありません"))?;
        m.insert(k.to_string(), v.clone());
        i += 2;
    }
    Ok(m)
}

fn wants(f: &HashMap<String, String>) -> Result<Vec<Want>, String> {
    let spec = f.get("os").ok_or("--os windows=200G,linux=100G,dreamos のように指定してください")?;
    spec.split(',')
        .map(|item| {
            let (name, size) = match item.split_once('=') {
                Some((n, s)) => (n, Some(parse_size(s)?)),
                None => (item, None),
            };
            Ok(Want { os: Os::parse(name)?, size_mib: size })
        })
        .collect()
}

fn options(f: &HashMap<String, String>) -> Result<Options, String> {
    let mut o = Options::default();
    if let Some(s) = f.get("esp") {
        o.esp_mib = parse_size(s)?;
    }
    if let Some(s) = f.get("shared") {
        o.shared_mib = Some(parse_size(s)?);
    }
    if let Some(s) = f.get("swap") {
        o.linux_swap_mib = Some(parse_size(s)?);
    }
    Ok(o)
}

fn default_os(f: &HashMap<String, String>, w: &[Want]) -> Result<Os, String> {
    match f.get("default") {
        Some(s) => Os::parse(s),
        None => Ok(w[0].os),
    }
}

fn read_tree(dir: &Path, prefix: &str, root: &mut Node) -> Result<(), String> {
    for e in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let e = e.map_err(|e| e.to_string())?;
        let name = e.file_name().to_string_lossy().to_string();
        let rel = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        let ft = e.file_type().map_err(|e| e.to_string())?;
        if ft.is_dir() {
            read_tree(&e.path(), &rel, root)?;
        } else if ft.is_file() {
            root.add_file(&rel, fs::read(e.path()).map_err(|e| e.to_string())?);
        }
    }
    Ok(())
}

fn refuse_dangerous_device(path: &str) -> Result<(), String> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with("physicaldrive0") {
        return Err("PhysicalDrive0 は通常システムディスクです。拒否します".into());
    }
    if let Ok(mounts) = fs::read_to_string("/proc/mounts") {
        if mounts.lines().any(|l| l.split_whitespace().next().map_or(false, |dev| dev.starts_with(path))) {
            return Err(format!("{path} (またはそのパーティション) はマウント中です。拒否します"));
        }
    }
    Ok(())
}

fn run(args: Vec<String>) -> Result<(), String> {
    let cmd = args.first().ok_or("コマンドがありません (plan / format / make-iso / grub-cfg / qemu)")?.as_str();
    let f = flags(&args[1..])?;
    match cmd {
        "plan" => {
            let w = wants(&f)?;
            let total = parse_size(f.get("disk").ok_or("--disk 500G")?)? * 1024 * 1024 / 512;
            let p = layout::plan(total, &w, &options(&f)?)?;
            print!("{}", layout::describe(&p));
        }
        "grub-cfg" => {
            let w = wants(&f)?;
            let oses: Vec<Os> = w.iter().map(|x| x.os).collect();
            print!("{}", bootcfg::grub_cfg(&oses, default_os(&f, &w)?, 5));
        }
        "format" => {
            let w = wants(&f)?;
            let opts = options(&f)?;
            let bl = f.get("bootloader").map(fs::read).transpose().map_err(|e| e.to_string())?;
            let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
            let (mut file, total, sparse) = if let Some(img) = f.get("image") {
                let bytes = parse_size(f.get("size").ok_or("--size 8G")?)? * 1024 * 1024;
                let file = fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(img).map_err(|e| e.to_string())?;
                file.set_len(bytes).map_err(|e| e.to_string())?;
                (file, bytes / 512, true)
            } else if let Some(dev) = f.get("device") {
                refuse_dangerous_device(dev)?;
                let n: u64 = f.get("size-bytes").ok_or("--size-bytes が必要です")?.parse().map_err(|e| format!("{e}"))?;
                let want = format!("ERASE {dev} {n}");
                if f.get("confirm").map(String::as_str) != Some(want.as_str()) {
                    return Err(format!("実ディスクを全消去します。続けるなら --confirm \"{want}\" を指定してください"));
                }
                let file = fs::OpenOptions::new().read(true).write(true).open(dev).map_err(|e| format!("{dev}: {e} (管理者権限/ディスクのオフライン化が必要)"))?;
                (file, n / 512, false)
            } else {
                return Err("--image か --device を指定してください".into());
            };
            let spec = DiskSpec { total_sectors: total, wants: &w, opts: &opts, default_os: default_os(&f, &w)?, seed, bootloader: bl.as_deref(), sparse };
            let r = media::format_disk(&mut file, &spec)?;
            println!("パーティション:\n{}", layout::describe(&r.plan));
            for s in &r.formatted {
                println!("フォーマット済み: {s}");
            }
            for s in &r.left_to_os {
                println!("後続: {s}");
            }
        }
        "make-iso" => {
            let w = wants(&f)?;
            let oses: Vec<Os> = w.iter().map(|x| x.os).collect();
            let mut root = Node::new_dir();
            read_tree(Path::new(f.get("payload").ok_or("--payload DIR")?), "", &mut root)?;
            let bl = f.get("bootloader").map(fs::read).transpose().map_err(|e| e.to_string())?;
            let iso = media::build_installer_iso(root, &oses, default_os(&f, &w)?, bl.as_deref(), "DREAMOS")?;
            let out = f.get("out").ok_or("--out dreamos.iso")?;
            fs::write(out, &iso).map_err(|e| e.to_string())?;
            println!("{out}: {} MiB{}", iso.len() >> 20, if bl.is_some() { " (UEFIブート可)" } else { " (ブートローダー未指定: データ専用。UEFIで起動するには --bootloader が必要)" });
        }
        "qemu" => {
            let host = Os::parse(f.get("host").ok_or("--host")?)?;
            let guest = Os::parse(f.get("guest").ok_or("--guest")?)?;
            println!("{}", bootcfg::qemu_command(host, guest, f.get("image").ok_or("--image")?, 4096, 2));
        }
        _ => return Err(format!("不明なコマンド: {cmd}")),
    }
    Ok(())
}

fn main() {
    if let Err(e) = run(std::env::args().skip(1).collect()) {
        eprintln!("エラー: {e}");
        std::process::exit(1);
    }
}
