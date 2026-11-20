//! Mã hóa/giải mã giao thức TRYX: "TRYX" + u32 LE độ dài + protobuf.

pub fn put_varint(buf: &mut Vec<u8>, mut v: u64) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            buf.push(b);
            return;
        }
        buf.push(b | 0x80);
    }
}

pub fn f_varint(f: u32, v: u64) -> Vec<u8> {
    let mut o = Vec::new();
    put_varint(&mut o, (f as u64) << 3);
    put_varint(&mut o, v);
    o
}

pub fn f_bytes(f: u32, d: &[u8]) -> Vec<u8> {
    let mut o = Vec::with_capacity(d.len() + 6);
    put_varint(&mut o, ((f as u64) << 3) | 2);
    put_varint(&mut o, d.len() as u64);
    o.extend_from_slice(d);
    o
}

pub fn f_str(f: u32, s: &str) -> Vec<u8> {
    f_bytes(f, s.as_bytes())
}

pub fn frame(payload: &[u8]) -> Vec<u8> {
    let mut o = Vec::with_capacity(payload.len() + 8);
    o.extend_from_slice(b"TRYX");
    o.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    o.extend_from_slice(payload);
    o
}

fn cat(parts: &[Vec<u8>]) -> Vec<u8> {
    parts.concat()
}

fn hdr_v1() -> Vec<u8> {
    f_bytes(1, &f_varint(1, 1))
}
fn hdr_empty() -> Vec<u8> {
    f_bytes(1, &[])
}

// Các message dưới đây đều đã đối chiếu từng byte với gói tin bắt được.
pub fn msg_handshake() -> Vec<u8> { cat(&[hdr_v1(), f_bytes(100, &f_str(1, "NA"))]) }
pub fn msg_hwconfig() -> Vec<u8> { cat(&[hdr_empty(), f_bytes(420, &f_varint(2, 18))]) }
pub fn msg_102() -> Vec<u8> { cat(&[hdr_v1(), f_bytes(102, &f_str(1, "NA"))]) }
pub fn msg_101() -> Vec<u8> { cat(&[hdr_v1(), f_bytes(101, &f_varint(1, 1))]) }
pub fn msg_time(ms: u64) -> Vec<u8> {
    cat(&[f_bytes(1, &f_varint(2, ms)), f_bytes(110, &f_str(1, "NA"))])
}
pub fn msg_list_files() -> Vec<u8> { cat(&[hdr_empty(), f_bytes(103, &f_str(1, "N/A"))]) }
pub fn msg_commit() -> Vec<u8> { cat(&[hdr_empty(), f_bytes(201, &[])]) }
pub fn msg_hello() -> Vec<u8> { cat(&[hdr_empty(), f_bytes(10, &f_str(1, "hello?"))]) }
pub fn msg_play(poweron: &str, standby: &str, playing: &str, level: u64) -> Vec<u8> {
    configured_play(poweron, standby, playing, level, &crate::config::Layout::default())
}

pub fn configured_play(poweron: &str, standby: &str, playing: &str, level: u64, layout: &crate::config::Layout) -> Vec<u8> {
    let mut work = f_str(3,playing);
    if layout.split { work.extend(cat(&[f_varint(1,1),f_str(4,&layout.left),f_str(5,&layout.right)])); }
    if layout.loop_mode != 0 {work.extend(f_varint(2,layout.loop_mode.min(2)));}
    let body = cat(&[
        f_bytes(1, &f_str(1, poweron)),
        f_bytes(2, &cat(&[f_varint(1, 1), f_str(2, standby)])),
        f_bytes(3, &work),
        f_bytes(5, &cat(&[f_varint(1, layout.enabled as u64), f_varint(2, level)])),
    ]);
    cat(&[hdr_empty(), f_bytes(200, &body)])
}

// ---------- giải mã protobuf không cần schema, in dạng cây ----------

fn overlay_group(layout: &crate::config::Layout) -> Vec<u8> {
    let mut group = cat(&[f_varint(1,1),f_varint(2,24),f_varint(3,24),f_varint(4,1000),f_varint(5,350),f_varint(6,layout.alignment.clamp(1,3))]);
    for (i, metric) in layout.metrics.iter().take(3).enumerate() {
        let label = cat(&[f_varint(1,i as u64+1),f_varint(2,i as u64),f_str(8,"roboto-regular"),f_varint(9,layout.font_size.clamp(18,64)),f_varint(10,if layout.dark_text {0} else {0xffffff}),f_str(11,metric)]);
        group.extend(f_bytes(8,&label));
    }
    group
}

pub fn overlay(layout: &crate::config::Layout) -> Vec<u8> {
    if layout.metrics.is_empty() {return msg_commit();}
    cat(&[hdr_empty(),f_bytes(201,&f_bytes(1,&overlay_group(layout)))])
}

fn varint(b: &[u8], i: &mut usize) -> Option<u64> {
    let (mut v, mut s) = (0u64, 0u32);
    loop {
        let c = *b.get(*i)?;
        *i += 1;
        if s >= 64 {
            return None;
        }
        v |= ((c & 0x7f) as u64) << s;
        s += 7;
        if c & 0x80 == 0 {
            return Some(v);
        }
    }
}

fn printable(d: &[u8]) -> bool {
    !d.is_empty() && d.iter().all(|&c| (0x20..0x7f).contains(&c))
}

fn walk(b: &[u8], ind: usize, depth: usize, out: &mut String) -> bool {
    let pad = " ".repeat(ind);
    let mut i = 0;
    while i < b.len() {
        let Some(k) = varint(b, &mut i) else { return false };
        let (f, w) = (k >> 3, k & 7);
        match w {
            0 => {
                let Some(v) = varint(b, &mut i) else { return false };
                out.push_str(&format!("{pad}{f}: {v}\n"));
            }
            2 => {
                let Some(l) = varint(b, &mut i) else { return false };
                let l = l as usize;
                if i + l > b.len() {
                    return false;
                }
                let d = &b[i..i + l];
                i += l;
                if printable(d) {
                    out.push_str(&format!("{pad}{f}: \"{}\"\n", String::from_utf8_lossy(d)));
                } else {
                    let mut sub = String::new();
                    if depth < 8 && !d.is_empty() && walk(d, ind + 2, depth + 1, &mut sub) {
                        out.push_str(&format!("{pad}{f}: {{\n{sub}{pad}}}\n"));
                    } else {
                        let hex: String = d.iter().take(48).map(|x| format!("{x:02x}")).collect();
                        out.push_str(&format!("{pad}{f}: hex {hex}\n"));
                    }
                }
            }
            1 => {
                if i + 8 > b.len() { return false; }
                out.push_str(&format!("{pad}{f}: f64 {}\n", hex(&b[i..i + 8])));
                i += 8;
            }
            5 => {
                if i + 4 > b.len() { return false; }
                out.push_str(&format!("{pad}{f}: f32 {}\n", hex(&b[i..i + 4])));
                i += 4;
            }
            _ => return false,
        }
    }
    true
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn pretty(b: &[u8]) -> String {
    let mut s = String::new();
    if !walk(b, 0, 0, &mut s) {
        s.push_str(&format!("(không giải mã được) hex {}\n", hex(b)));
    }
    s
}

// ---------- phân tích protobuf có cấu trúc ----------

#[derive(Debug, Clone, Copy)]
pub enum Val<'a> {
    Int(u64),
    Bytes(&'a [u8]),
    Other,
}

pub fn parse(b: &[u8]) -> Option<Vec<(u32, Val<'_>)>> {
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let k = varint(b, &mut i)?;
        let f = (k >> 3) as u32;
        match k & 7 {
            0 => out.push((f, Val::Int(varint(b, &mut i)?))),
            2 => {
                let l = varint(b, &mut i)? as usize;
                let end = i.checked_add(l)?;
                if end > b.len() {
                    return None;
                }
                out.push((f, Val::Bytes(&b[i..end])));
                i = end;
            }
            1 => {
                if i + 8 > b.len() {
                    return None;
                }
                out.push((f, Val::Other));
                i += 8;
            }
            5 => {
                if i + 4 > b.len() {
                    return None;
                }
                out.push((f, Val::Other));
                i += 4;
            }
            _ => return None,
        }
    }
    Some(out)
}

pub fn find_bytes<'a>(fields: &[(u32, Val<'a>)], n: u32) -> Option<&'a [u8]> {
    fields.iter().find_map(|(k, v)| match v {
        Val::Bytes(b) if *k == n => Some(*b),
        _ => None,
    })
}

pub fn find_str(fields: &[(u32, Val<'_>)], n: u32) -> String {
    find_bytes(fields, n)
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default()
}

pub fn find_int(fields: &[(u32, Val<'_>)], n: u32) -> Option<u64> {
    fields.iter().find_map(|(k, v)| match v {
        Val::Int(x) if *k == n => Some(*x),
        _ => None,
    })
}

fn collect_strings(b: &[u8], out: &mut Vec<String>, depth: usize) {
    let Some(fields) = parse(b) else { return };
    for (_, v) in fields {
        if let Val::Bytes(d) = v {
            if printable(d) {
                out.push(String::from_utf8_lossy(d).into_owned());
            } else if depth < 8 {
                collect_strings(d, out, depth + 1);
            }
        }
    }
}

/// Lấy mọi đường dẫn file ("/mnt/data/...") trong phản hồi.
pub fn find_paths(b: &[u8]) -> Vec<String> {
    let mut v = Vec::new();
    collect_strings(b, &mut v, 0);
    v.into_iter()
        .filter(|p| p.starts_with("/mnt/data/") && p.contains('.'))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn h(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }
    #[test]
    fn split_layout_and_metrics() {
        let layout = crate::config::Layout { split:true, left:"left.mp4".into(), right:"right.mp4".into(), loop_mode:2,
            metrics:vec!["CPU".into(),"RAM".into()], ..Default::default() };
        let msg = configured_play("on","off","full",67,&layout);
        let request=parse(&msg).unwrap();
        let config=parse(find_bytes(&request,200).unwrap()).unwrap();
        let work=parse(find_bytes(&config,3).unwrap()).unwrap();
        assert_eq!(find_int(&work,1),Some(1));
        assert_eq!(find_int(&work,2),Some(2));
        assert_eq!(find_str(&work,4),"left.mp4");
        assert_eq!(find_str(&work,5),"right.mp4");
        let msg=overlay(&layout);
        let req=parse(&msg).unwrap();
        let overlay=parse(find_bytes(&req,201).unwrap()).unwrap();
        let group=parse(find_bytes(&overlay,1).unwrap()).unwrap();
        assert_eq!(group.iter().filter(|(field,_)| *field==8).count(),2);
    }

    #[test]
    fn khop_goi_tin_bat_duoc() {
        assert_eq!(frame(&msg_handshake()), h("545259580b0000000a020801a206040a024e41"));
        assert_eq!(frame(&msg_hwconfig()), h("54525958070000000a00a21a021012"));
        assert_eq!(frame(&msg_hello()), h("545259580c0000000a0052080a0668656c6c6f3f"));
        assert_eq!(
            frame(&msg_time(1795139178756)),
            h("54525958100000000a071084a288b59f34f206040a024e41")
        );
        assert_eq!(
            frame(&msg_play(
                "default_poweron.mp4.h264_2240x1080",
                "default_standby.mp4.h264_2240x1080",
                "download_46.mp4.h264_2240x1080",
                67
            )),
            h("545259587b0000000a00c20c760a240a2264656661756c745f706f7765726f6e2e6d70342e683236345f32323430783130383012260801122264656661756c745f7374616e6462792e6d70342e683236345f3232343078313038301a201a1e646f776e6c6f61645f34362e6d70342e683236345f3232343078313038302a0408011043")
        );
    }
}
