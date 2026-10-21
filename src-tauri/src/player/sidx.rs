//! Segment index of a YouTube DASH MP4: `ftyp`, `moov`, `sidx`, then
//! `moof`/`mdat` pairs. The `sidx` box maps time to byte ranges, which is all
//! the MSE player needs to fetch and seek. yt-dlp doesn't expose the ranges,
//! so we read the head of the file ourselves.

use serde::Serialize;

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    /// Byte range, inclusive — ready for a `Range: bytes=start-end` header.
    pub start: u64,
    pub end: u64,
    /// Seconds.
    pub t: f64,
    pub d: f64,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Index {
    /// Inclusive end of the init segment (`ftyp` + `moov`).
    pub init_end: u64,
    pub segments: Vec<Segment>,
}

#[derive(Debug, PartialEq)]
pub enum Parse {
    Done(Index),
    /// The buffer ends before the index does; fetch this many bytes from 0.
    NeedBytes(u64),
}

fn be(b: &[u8]) -> u64 {
    b.iter().fold(0, |acc, &x| (acc << 8) | x as u64)
}

pub fn parse(buf: &[u8]) -> Result<Parse, &'static str> {
    let mut pos = 0u64;
    let mut init_end = None;
    loop {
        let p = pos as usize;
        if p + 8 > buf.len() {
            return Ok(Parse::NeedBytes(pos + 16 * 1024));
        }
        let mut size = be(&buf[p..p + 4]);
        let kind = &buf[p + 4..p + 8];
        let mut header = 8u64;
        if size == 1 {
            if p + 16 > buf.len() {
                return Ok(Parse::NeedBytes(pos + 16 * 1024));
            }
            size = be(&buf[p + 8..p + 16]);
            header = 16;
        }
        if size < header {
            return Err("broken MP4 box");
        }
        let end = pos + size;
        match kind {
            b"moov" => init_end = Some(end - 1),
            b"sidx" => {
                let init_end = init_end.ok_or("sidx before moov")?;
                if end as usize > buf.len() {
                    return Ok(Parse::NeedBytes(end));
                }
                let segments = parse_sidx(&buf[(pos + header) as usize..end as usize], end)?;
                return Ok(Parse::Done(Index { init_end, segments }));
            }
            b"moof" | b"mdat" => return Err("no segment index in this stream"),
            _ => {}
        }
        pos = end;
    }
}

/// `body` is the box payload, `box_end` the file offset right after the box.
fn parse_sidx(body: &[u8], box_end: u64) -> Result<Vec<Segment>, &'static str> {
    const SHORT: &str = "truncated sidx";
    let version = *body.first().ok_or(SHORT)?;
    // version(1) flags(3) reference_ID(4) timescale(4)
    let timescale = be(body.get(8..12).ok_or(SHORT)?) as f64;
    let (mut t, first_offset, mut p) = if version == 0 {
        (be(body.get(12..16).ok_or(SHORT)?), be(body.get(16..20).ok_or(SHORT)?), 20)
    } else {
        (be(body.get(12..20).ok_or(SHORT)?), be(body.get(20..28).ok_or(SHORT)?), 28)
    };
    if timescale == 0.0 {
        return Err("sidx timescale is 0");
    }
    p += 2; // reserved
    let count = be(body.get(p..p + 2).ok_or(SHORT)?) as usize;
    p += 2;

    let mut offset = box_end + first_offset;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let r = body.get(p..p + 12).ok_or(SHORT)?;
        let word = be(&r[0..4]);
        if word >> 31 == 1 {
            return Err("nested sidx is not supported");
        }
        let size = word & 0x7fff_ffff;
        let dur = be(&r[4..8]);
        out.push(Segment { start: offset, end: offset + size - 1, t: t as f64 / timescale, d: dur as f64 / timescale });
        offset += size;
        t += dur;
        p += 12;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bx(kind: &[u8], body: &[u8]) -> Vec<u8> {
        let mut v = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        v.extend_from_slice(kind);
        v.extend_from_slice(body);
        v
    }

    fn sidx_v0(timescale: u32, ept: u32, first: u32, refs: &[(u32, u32)]) -> Vec<u8> {
        let mut b = vec![0, 0, 0, 0, 0, 0, 0, 1];
        b.extend(timescale.to_be_bytes());
        b.extend(ept.to_be_bytes());
        b.extend(first.to_be_bytes());
        b.extend([0, 0]);
        b.extend((refs.len() as u16).to_be_bytes());
        for &(size, dur) in refs {
            b.extend(size.to_be_bytes());
            b.extend(dur.to_be_bytes());
            b.extend(0x9000_0000u32.to_be_bytes());
        }
        bx(b"sidx", &b)
    }

    #[test]
    fn parses_youtube_layout() {
        let mut f = bx(b"ftyp", &[0; 16]);
        f.extend(bx(b"moov", &[0; 100]));
        let init_end = f.len() as u64 - 1;
        f.extend(sidx_v0(1000, 0, 0, &[(500, 5000), (700, 4000)]));
        let anchor = f.len() as u64;
        let Parse::Done(ix) = parse(&f).unwrap() else { panic!("need more") };
        assert_eq!(ix.init_end, init_end);
        assert_eq!(
            ix.segments,
            vec![
                Segment { start: anchor, end: anchor + 499, t: 0.0, d: 5.0 },
                Segment { start: anchor + 500, end: anchor + 1199, t: 5.0, d: 4.0 },
            ]
        );
    }

    #[test]
    fn asks_for_more_when_sidx_is_cut() {
        let mut f = bx(b"ftyp", &[0; 16]);
        f.extend(bx(b"moov", &[0; 100]));
        let full = f.len() + sidx_v0(1000, 0, 0, &[(1, 1); 10]).len();
        f.extend(sidx_v0(1000, 0, 0, &[(1, 1); 10]));
        assert_eq!(parse(&f[..full - 5]).unwrap(), Parse::NeedBytes(full as u64));
    }

    #[test]
    fn rejects_files_without_index() {
        let mut f = bx(b"ftyp", &[0; 16]);
        f.extend(bx(b"moov", &[0; 10]));
        f.extend(bx(b"moof", &[0; 10]));
        assert!(parse(&f).is_err());
    }

    /// Dev check on a real file: `SIDX_FILE=path cargo test sidx::tests::real_file -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_file() {
        let path = std::env::var("SIDX_FILE").expect("SIDX_FILE");
        let buf = std::fs::read(path).unwrap();
        match parse(&buf).unwrap() {
            Parse::Done(index) => println!("{}", serde_json::to_string(&index).unwrap()),
            Parse::NeedBytes(n) => panic!("needs {n} bytes"),
        }
    }
}
