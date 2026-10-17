//! ISO9660 + Joliet(長い名前・日本語名) + El Torito(UEFIブート)のイメージ生成。CD/DVDに焼ける。
//! UEFIブートイメージにはFAT32のESPイメージ(`fat32`モジュール)をそのまま使う。

use crate::Stamp;

const SS: usize = 2048;

pub enum Node {
    Dir(Vec<(String, Node)>),
    File(Vec<u8>),
}

impl Node {
    pub fn new_dir() -> Node {
        Node::Dir(Vec::new())
    }

    /// `a/b/c.txt`のパスにファイルを追加する。
    pub fn add_file(&mut self, path: &str, data: Vec<u8>) {
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        let Node::Dir(children) = self else { return };
        let (first, rest) = parts.split_first().expect("空のパス");
        if rest.is_empty() {
            children.retain(|(n, _)| n != first);
            children.push((first.to_string(), Node::File(data)));
        } else {
            let pos = children.iter().position(|(n, c)| n == first && matches!(c, Node::Dir(_)));
            let idx = pos.unwrap_or_else(|| {
                children.push((first.to_string(), Node::new_dir()));
                children.len() - 1
            });
            children[idx].1.add_file(&rest.join("/"), data);
        }
    }
}

pub struct IsoOptions {
    pub volume_id: String,
    /// UEFI El Toritoブートイメージ(FAT32 ESPイメージ)。`None`ならデータ専用。
    pub efi_boot_image: Option<Vec<u8>>,
    pub stamp: Stamp,
}

struct FlatFile {
    data: Vec<u8>,
    lba: u32,
}

struct FlatDir {
    /// このツリー内での名前(識別子バイト列)と種類
    children: Vec<(Vec<u8>, Child)>,
    parent: usize,
    lba: u32,
    size: u32,
}

enum Child {
    Dir(usize),
    File(usize),
}

fn both32(v: u32) -> [u8; 8] {
    let mut b = [0u8; 8];
    b[0..4].copy_from_slice(&v.to_le_bytes());
    b[4..8].copy_from_slice(&v.to_be_bytes());
    b
}

fn both16(v: u16) -> [u8; 4] {
    let mut b = [0u8; 4];
    b[0..2].copy_from_slice(&v.to_le_bytes());
    b[2..4].copy_from_slice(&v.to_be_bytes());
    b
}

fn iso_char(c: char) -> char {
    let u = c.to_ascii_uppercase();
    if u.is_ascii_uppercase() || u.is_ascii_digit() || u == '_' { u } else { '_' }
}

fn iso_name(name: &str, is_dir: bool, taken: &mut Vec<String>) -> String {
    let (base, ext) = match name.rfind('.') {
        Some(i) if !is_dir && i > 0 => (&name[..i], &name[i + 1..]),
        _ => (name, ""),
    };
    let b: String = base.chars().filter(|c| *c != '.').map(iso_char).take(8).collect();
    let e: String = ext.chars().map(iso_char).take(3).collect();
    let mk = |b: &str| if is_dir { b.to_string() } else { format!("{b}.{e};1") };
    let mut cand = mk(&b);
    let mut n = 1;
    while taken.contains(&cand) {
        let t = format!("~{n}");
        let keep: String = b.chars().take(8 - t.len()).collect();
        cand = mk(&format!("{keep}{t}"));
        n += 1;
    }
    taken.push(cand.clone());
    cand
}

fn utf16be(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(|u| u.to_be_bytes()).collect()
}

fn dir_record(extent: u32, len: u32, is_dir: bool, ident: &[u8], stamp: &Stamp) -> Vec<u8> {
    let pad = if ident.len() % 2 == 0 { 1 } else { 0 };
    let total = 33 + ident.len() + pad;
    let mut r = vec![0u8; total];
    r[0] = total as u8;
    r[2..10].copy_from_slice(&both32(extent));
    r[10..18].copy_from_slice(&both32(len));
    r[18..25].copy_from_slice(&stamp.iso7());
    r[25] = if is_dir { 2 } else { 0 };
    r[28..32].copy_from_slice(&both16(1));
    r[32] = ident.len() as u8;
    r[33..33 + ident.len()].copy_from_slice(ident);
    r
}

/// ノード木をBFSで平坦化する。`name_of`で識別子バイト列を決める。
fn flatten(
    root: &Node,
    files: &mut Vec<Vec<u8>>,
    file_ids: &mut Vec<*const Vec<u8>>,
    joliet: bool,
) -> Vec<FlatDir> {
    let mut dirs: Vec<FlatDir> = vec![FlatDir { children: vec![], parent: 0, lba: 0, size: 0 }];
    let mut queue: Vec<(&Node, usize)> = vec![(root, 0)];
    let mut qi = 0;
    while qi < queue.len() {
        let (node, di) = queue[qi];
        qi += 1;
        let Node::Dir(kids) = node else { continue };
        let mut taken = Vec::new();
        let mut out: Vec<(Vec<u8>, Child)> = Vec::new();
        for (name, child) in kids {
            let is_dir = matches!(child, Node::Dir(_));
            let ident = if joliet {
                utf16be(&name.chars().take(100).collect::<String>())
            } else {
                iso_name(name, is_dir, &mut taken).into_bytes()
            };
            match child {
                Node::Dir(_) => {
                    dirs.push(FlatDir { children: vec![], parent: di, lba: 0, size: 0 });
                    let id = dirs.len() - 1;
                    queue.push((child, id));
                    out.push((ident, Child::Dir(id)));
                }
                Node::File(d) => {
                    let ptr = d as *const Vec<u8>;
                    let id = match file_ids.iter().position(|p| *p == ptr) {
                        Some(i) => i,
                        None => {
                            file_ids.push(ptr);
                            files.push(d.clone());
                            files.len() - 1
                        }
                    };
                    out.push((ident, Child::File(id)));
                }
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        dirs[di].children = out;
    }
    dirs
}

fn dir_bytes(dirs: &[FlatDir], i: usize, files: &[FlatFile], stamp: &Stamp) -> Vec<u8> {
    let d = &dirs[i];
    let mut recs: Vec<Vec<u8>> = vec![
        dir_record(d.lba, d.size, true, &[0], stamp),
        dir_record(dirs[d.parent].lba, dirs[d.parent].size, true, &[1], stamp),
    ];
    for (ident, c) in &d.children {
        recs.push(match c {
            Child::Dir(id) => dir_record(dirs[*id].lba, dirs[*id].size, true, ident, stamp),
            Child::File(id) => dir_record(files[*id].lba, files[*id].data.len() as u32, false, ident, stamp),
        });
    }
    let mut out: Vec<u8> = Vec::new();
    for r in recs {
        let used = out.len() % SS;
        if used + r.len() > SS {
            out.resize(out.len() + (SS - used), 0);
        }
        out.extend_from_slice(&r);
    }
    out.resize(out.len().div_ceil(SS).max(1) * SS, 0);
    out
}

fn path_table(dirs: &[FlatDir], big: bool) -> Vec<u8> {
    let w32 = |v: u32| if big { v.to_be_bytes() } else { v.to_le_bytes() };
    let w16 = |v: u16| if big { v.to_be_bytes() } else { v.to_le_bytes() };
    let mut idents: Vec<Vec<u8>> = vec![vec![0]; dirs.len()];
    for (pi, d) in dirs.iter().enumerate() {
        for (ident, c) in &d.children {
            if let Child::Dir(id) = c {
                idents[*id] = ident.clone();
            }
        }
        let _ = pi;
    }
    let mut t = Vec::new();
    for (i, d) in dirs.iter().enumerate() {
        let id = &idents[i];
        t.push(id.len() as u8);
        t.push(0);
        t.extend_from_slice(&w32(d.lba));
        t.extend_from_slice(&w16(if i == 0 { 1 } else { d.parent as u16 + 1 }));
        t.extend_from_slice(id);
        if id.len() % 2 == 1 {
            t.push(0);
        }
    }
    t
}

fn pad_str(s: &str, n: usize) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    v.truncate(n);
    v.resize(n, b' ');
    v
}

fn volume_descriptor(
    kind: u8,
    vol: &str,
    joliet: bool,
    total_sectors: u32,
    pt_size: u32,
    pt_l: u32,
    pt_m: u32,
    root: Vec<u8>,
    stamp: &Stamp,
) -> Vec<u8> {
    let mut d = vec![0u8; SS];
    d[0] = kind;
    d[1..6].copy_from_slice(b"CD001");
    d[6] = 1;
    let text = |s: &str, n: usize| if joliet {
        let mut u = utf16be(s);
        u.truncate(n);
        while u.len() < n {
            u.extend_from_slice(&[0x00, 0x20]);
        }
        u
    } else {
        pad_str(s, n)
    };
    d[8..40].copy_from_slice(&text("DREAMOS", 32));
    d[40..72].copy_from_slice(&text(&vol.to_ascii_uppercase(), 32));
    d[80..88].copy_from_slice(&both32(total_sectors));
    if joliet {
        d[88..91].copy_from_slice(b"%/E");
    }
    d[120..124].copy_from_slice(&both16(1));
    d[124..128].copy_from_slice(&both16(1));
    d[128..132].copy_from_slice(&both16(SS as u16));
    d[132..140].copy_from_slice(&both32(pt_size));
    d[140..144].copy_from_slice(&pt_l.to_le_bytes());
    d[148..152].copy_from_slice(&pt_m.to_be_bytes());
    d[156..190].copy_from_slice(&root[..34]);
    for off in [190usize, 318, 446, 574] {
        d[off..off + 128].copy_from_slice(&text("", 128));
    }
    for off in [702usize, 739, 776] {
        d[off..off + 37].fill(b' ');
    }
    let s = stamp.iso17();
    for off in [813usize, 830, 864] {
        d[off..off + 17].copy_from_slice(&s);
    }
    d[847..864].copy_from_slice(b"0000000000000000\0");
    d[881] = 1;
    d
}

pub fn build(root: &Node, opts: &IsoOptions) -> Result<Vec<u8>, String> {
    // ファイル本体(両ツリーで共有)
    let mut file_data: Vec<Vec<u8>> = Vec::new();
    let mut ids: Vec<*const Vec<u8>> = Vec::new();
    let mut pdirs = flatten(root, &mut file_data, &mut ids, false);
    let mut jdirs = flatten(root, &mut file_data, &mut ids, true);
    let mut files: Vec<FlatFile> = file_data.into_iter().map(|data| FlatFile { data, lba: 0 }).collect();

    // 配置: 16 PVD / 17 BRVD / 18 SVD(Joliet) / 19 終端 / 20 ブートカタログ / 以降
    let mut next: u32 = 21;
    let mut alloc = |bytes: usize| -> u32 {
        let l = next;
        next += bytes.div_ceil(SS).max(1) as u32;
        l
    };
    // ディレクトリ(サイズは内容次第なので、仮配置してサイズを確定する)
    for dirs in [&mut pdirs, &mut jdirs] {
        // ディレクトリのバイト長は子のLBAに依存しないので、先にサイズだけ求める
        for i in 0..dirs.len() {
            let sz = dir_size(dirs, i);
            dirs[i].size = sz as u32;
        }
        for i in 0..dirs.len() {
            dirs[i].lba = alloc(dirs[i].size as usize);
        }
    }
    let ptsize_p = path_table(&pdirs, false).len();
    let ptsize_j = path_table(&jdirs, false).len();
    let pt_p_l = alloc(ptsize_p);
    let pt_p_m = alloc(ptsize_p);
    let pt_j_l = alloc(ptsize_j);
    let pt_j_m = alloc(ptsize_j);
    for f in files.iter_mut() {
        f.lba = alloc(f.data.len());
    }
    let efi_lba = opts.efi_boot_image.as_ref().map(|img| alloc(img.len()));
    let total = next;

    let mut out = vec![0u8; total as usize * SS];
    let put = |out: &mut Vec<u8>, lba: u32, data: &[u8]| {
        let o = lba as usize * SS;
        out[o..o + data.len()].copy_from_slice(data);
    };
    let st = &opts.stamp;
    let root_p = dir_record(pdirs[0].lba, pdirs[0].size, true, &[0], st);
    let root_j = dir_record(jdirs[0].lba, jdirs[0].size, true, &[0], st);
    put(&mut out, 16, &volume_descriptor(1, &opts.volume_id, false, total, ptsize_p as u32, pt_p_l, pt_p_m, root_p, st));
    if let Some(_) = efi_lba {
        let mut b = vec![0u8; SS];
        b[1..6].copy_from_slice(b"CD001");
        b[6] = 1;
        b[7..30].copy_from_slice(b"EL TORITO SPECIFICATION");
        b[71..75].copy_from_slice(&20u32.to_le_bytes());
        put(&mut out, 17, &b);
    }
    let svd_lba = if efi_lba.is_some() { 18 } else { 17 };
    put(&mut out, svd_lba, &volume_descriptor(2, &opts.volume_id, true, total, ptsize_j as u32, pt_j_l, pt_j_m, root_j, st));
    let mut term = vec![0u8; SS];
    term[0] = 255;
    term[1..6].copy_from_slice(b"CD001");
    term[6] = 1;
    put(&mut out, svd_lba + 1, &term);

    if let (Some(img), Some(lba)) = (&opts.efi_boot_image, efi_lba) {
        let mut cat = vec![0u8; SS];
        cat[0] = 1;
        cat[1] = 0xEF; // UEFI
        let id = b"DREAMOS";
        cat[4..4 + id.len()].copy_from_slice(id);
        cat[30] = 0x55;
        cat[31] = 0xAA;
        let mut sum: u16 = 0;
        for i in 0..16 {
            sum = sum.wrapping_add(u16::from_le_bytes([cat[i * 2], cat[i * 2 + 1]]));
        }
        cat[28..30].copy_from_slice(&0u16.wrapping_sub(sum).to_le_bytes());
        cat[32] = 0x88; // bootable
        cat[33] = 0; // no emulation
        let count = (img.len() / 512).min(0xFFFF) as u16;
        cat[38..40].copy_from_slice(&count.to_le_bytes());
        cat[40..44].copy_from_slice(&lba.to_le_bytes());
        put(&mut out, 20, &cat);
        put(&mut out, lba, img);
    }

    for dirs in [&pdirs, &jdirs] {
        for i in 0..dirs.len() {
            let b = dir_bytes(dirs, i, &files, st);
            put(&mut out, dirs[i].lba, &b);
        }
    }
    put(&mut out, pt_p_l, &path_table(&pdirs, false));
    put(&mut out, pt_p_m, &path_table(&pdirs, true));
    put(&mut out, pt_j_l, &path_table(&jdirs, false));
    put(&mut out, pt_j_m, &path_table(&jdirs, true));
    for f in &files {
        put(&mut out, f.lba, &f.data);
    }
    Ok(out)
}

fn dir_size(dirs: &[FlatDir], i: usize) -> usize {
    let d = &dirs[i];
    let mut lens = vec![34 + 0usize, 34];
    for (ident, _) in &d.children {
        lens.push(33 + ident.len() + if ident.len() % 2 == 0 { 1 } else { 0 });
    }
    let mut used = 0usize;
    let mut sectors = 1usize;
    for l in lens {
        if used + l > SS {
            sectors += 1;
            used = 0;
        }
        used += l;
    }
    sectors * SS
}

/// 検証用の最小リーダー: Joliet/ISO9660ツリーのファイルを取り出す(`joliet`で選択)。
pub fn read_file(iso: &[u8], path: &str, joliet: bool) -> Option<Vec<u8>> {
    let mut vd = None;
    for s in 16..40usize {
        let o = s * SS;
        if o + SS > iso.len() || &iso[o + 1..o + 6] != b"CD001" {
            return None;
        }
        if iso[o] == 255 {
            break;
        }
        if iso[o] == (if joliet { 2 } else { 1 }) {
            vd = Some(o);
        }
    }
    let o = vd?;
    let rec = &iso[o + 156..o + 190];
    let mut ext = u32::from_le_bytes(rec[2..6].try_into().unwrap());
    let mut len = u32::from_le_bytes(rec[10..14].try_into().unwrap());
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    for (pi, part) in parts.iter().enumerate() {
        let dir = &iso[ext as usize * SS..ext as usize * SS + len as usize];
        let mut p = 0usize;
        let mut found = None;
        while p < dir.len() {
            let rl = dir[p] as usize;
            if rl == 0 {
                p = (p / SS + 1) * SS;
                continue;
            }
            let idl = dir[p + 32] as usize;
            let id = &dir[p + 33..p + 33 + idl];
            let name = if joliet {
                let u: Vec<u16> = id.chunks(2).map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect();
                String::from_utf16_lossy(&u)
            } else {
                String::from_utf8_lossy(id).split(';').next().unwrap_or("").trim_end_matches('.').to_string()
            };
            if idl > 1 || (idl == 1 && id[0] > 1) {
                let want = if joliet { part.to_string() } else { part.to_ascii_uppercase() };
                if name == want {
                    found = Some((
                        u32::from_le_bytes(dir[p + 2..p + 6].try_into().unwrap()),
                        u32::from_le_bytes(dir[p + 10..p + 14].try_into().unwrap()),
                        dir[p + 25] & 2 != 0,
                    ));
                    break;
                }
            }
            p += rl;
        }
        let (e, l, is_dir) = found?;
        if pi + 1 == parts.len() {
            return if is_dir { None } else { Some(iso[e as usize * SS..e as usize * SS + l as usize].to_vec()) };
        }
        ext = e;
        len = l;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Node {
        let mut r = Node::new_dir();
        r.add_file("README.TXT", b"hello".to_vec());
        r.add_file("installer/dream-os-installer.exe", vec![7u8; 5000]);
        r.add_file("installer/説明書.txt", "日本語".as_bytes().to_vec());
        r.add_file("installer/a/b/deep.bin", vec![1, 2, 3]);
        r
    }

    #[test]
    fn joliet_and_plain_trees_read_back() {
        let iso = build(&sample(), &IsoOptions { volume_id: "DREAMOS".into(), efi_boot_image: None, stamp: Stamp::fixed() }).unwrap();
        assert_eq!(iso.len() % SS, 0);
        assert_eq!(&iso[16 * SS + 1..16 * SS + 6], b"CD001");
        assert_eq!(read_file(&iso, "installer/dream-os-installer.exe", true).unwrap(), vec![7u8; 5000]);
        assert_eq!(read_file(&iso, "installer/説明書.txt", true).unwrap(), "日本語".as_bytes());
        assert_eq!(read_file(&iso, "installer/a/b/deep.bin", true).unwrap(), vec![1, 2, 3]);
        assert_eq!(read_file(&iso, "readme.txt", false).unwrap(), b"hello");
        assert_eq!(read_file(&iso, "INSTALLE/DREAM_OS.EXE", false).unwrap().len(), 5000);
    }

    #[test]
    fn el_torito_catalog_is_valid() {
        let esp = vec![0xEBu8; 1024 * 1024];
        let iso = build(&sample(), &IsoOptions { volume_id: "DREAMOS".into(), efi_boot_image: Some(esp.clone()), stamp: Stamp::fixed() }).unwrap();
        assert_eq!(&iso[17 * SS + 7..17 * SS + 30], b"EL TORITO SPECIFICATION");
        let cat = &iso[20 * SS..20 * SS + 64];
        assert_eq!(cat[0], 1);
        assert_eq!(cat[1], 0xEF);
        assert_eq!(&cat[30..32], &[0x55, 0xAA]);
        let sum = (0..16).fold(0u16, |s, i| s.wrapping_add(u16::from_le_bytes([cat[i * 2], cat[i * 2 + 1]])));
        assert_eq!(sum, 0);
        assert_eq!(cat[32], 0x88);
        let lba = u32::from_le_bytes(cat[40..44].try_into().unwrap()) as usize;
        assert_eq!(&iso[lba * SS..lba * SS + esp.len()], esp.as_slice());
    }
}
