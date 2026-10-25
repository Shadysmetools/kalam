//! The supervisor ↔ worker wire format (over the worker's stdin/stdout pipes).
//!
//! Request: `skin: u8`, `len: u32 LE`, `len` bytes of UTF-8 text.
//! Reply: `tag: u8`, then
//! - `0` OK: `features`;
//! - `1` panic: `message`, `location`;
//! - `2` oracle violation: `name`, `detail`, `features`;
//!
//! where a string is `len: u32 LE` + bytes and `features` is `n: u32 LE` + `n` × `u32 LE`.

use std::io::{self, Read, Write};

use kalam_base::Skin;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    Ok {
        features: Vec<u32>,
    },
    Panic {
        message: String,
        location: String,
    },
    Oracle {
        name: String,
        detail: String,
        features: Vec<u32>,
    },
}

fn skin_byte(s: Skin) -> u8 {
    match s {
        Skin::En => 0,
        Skin::Ar => 1,
        Skin::Franco => 2,
    }
}

fn byte_skin(b: u8) -> Skin {
    match b {
        1 => Skin::Ar,
        2 => Skin::Franco,
        _ => Skin::En,
    }
}

fn put_u32(w: &mut impl Write, v: u32) -> io::Result<()> {
    w.write_all(&v.to_le_bytes())
}

fn get_u32(r: &mut impl Read) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn put_str(w: &mut impl Write, s: &str) -> io::Result<()> {
    put_u32(w, s.len() as u32)?;
    w.write_all(s.as_bytes())
}

fn get_str(r: &mut impl Read) -> io::Result<String> {
    let n = get_u32(r)? as usize;
    let mut b = vec![0u8; n];
    r.read_exact(&mut b)?;
    Ok(String::from_utf8_lossy(&b).into_owned())
}

fn put_features(w: &mut impl Write, f: &[u32]) -> io::Result<()> {
    put_u32(w, f.len() as u32)?;
    for v in f {
        put_u32(w, *v)?;
    }
    Ok(())
}

fn get_features(r: &mut impl Read) -> io::Result<Vec<u32>> {
    let n = get_u32(r)? as usize;
    (0..n.min(1 << 20)).map(|_| get_u32(r)).collect()
}

pub fn write_request(w: &mut impl Write, skin: Skin, text: &str) -> io::Result<()> {
    w.write_all(&[skin_byte(skin)])?;
    put_str(w, text)?;
    w.flush()
}

/// `Ok(None)` at a clean end of stream.
pub fn read_request(r: &mut impl Read) -> io::Result<Option<(Skin, String)>> {
    let mut tag = [0u8; 1];
    match r.read_exact(&mut tag) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let text = get_str(r)?;
    Ok(Some((byte_skin(tag[0]), text)))
}

pub fn write_reply(w: &mut impl Write, reply: &Reply) -> io::Result<()> {
    match reply {
        Reply::Ok { features } => {
            w.write_all(&[0])?;
            put_features(w, features)?;
        }
        Reply::Panic { message, location } => {
            w.write_all(&[1])?;
            put_str(w, message)?;
            put_str(w, location)?;
        }
        Reply::Oracle {
            name,
            detail,
            features,
        } => {
            w.write_all(&[2])?;
            put_str(w, name)?;
            put_str(w, detail)?;
            put_features(w, features)?;
        }
    }
    w.flush()
}

pub fn read_reply(r: &mut impl Read) -> io::Result<Reply> {
    let mut tag = [0u8; 1];
    r.read_exact(&mut tag)?;
    match tag[0] {
        0 => Ok(Reply::Ok {
            features: get_features(r)?,
        }),
        1 => Ok(Reply::Panic {
            message: get_str(r)?,
            location: get_str(r)?,
        }),
        2 => Ok(Reply::Oracle {
            name: get_str(r)?,
            detail: get_str(r)?,
            features: get_features(r)?,
        }),
        t => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("bad reply tag {t}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip() {
        let mut buf = Vec::new();
        write_request(&mut buf, Skin::Ar, "دالة x").unwrap();
        let mut r = &buf[..];
        assert_eq!(
            read_request(&mut r).unwrap(),
            Some((Skin::Ar, "دالة x".into()))
        );
        assert_eq!(read_request(&mut r).unwrap(), None);

        for reply in [
            Reply::Ok {
                features: vec![1, 2, 3],
            },
            Reply::Panic {
                message: "boom".into(),
                location: "a.rs:1".into(),
            },
            Reply::Oracle {
                name: "cst_lossless".into(),
                detail: "d".into(),
                features: vec![],
            },
        ] {
            let mut buf = Vec::new();
            write_reply(&mut buf, &reply).unwrap();
            assert_eq!(read_reply(&mut &buf[..]).unwrap(), reply);
        }
    }
}
