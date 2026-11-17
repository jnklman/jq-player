//! MPEG audio (MP3, MP2) inside a WAV file, as broadcast software and some
//! DAWs write it: format tag 0x0055 or 0x0050 (also as the sub-format of
//! WAVE_FORMAT_EXTENSIBLE). symphonia's WAV reader refuses these, so the
//! `data` chunk is handed to its MPEG reader on its own.
//!
//! Only used after symphonia has refused a file (see [`crate::open_format`]):
//! should it learn these files, this module goes unused, and the test
//! `symphonia_still_refuses_mp3_in_wav` in `tests/formats.rs` says so.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use symphonia::core::io::MediaSource;

const WAVE_FORMAT_MPEG: u16 = 0x0050;
const WAVE_FORMAT_MPEGLAYER3: u16 = 0x0055;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xfffe;

/// The `data` chunk of `path` as a source of its own, if `path` is a WAV
/// file holding MPEG audio. `None` for any other file.
pub(crate) fn mpeg_data(path: &Path) -> io::Result<Option<Window>> {
    let mut file = File::open(path)?;
    let Some((start, len)) = find_mpeg_data(&mut file)? else { return Ok(None) };
    // A file cut short (or written while recording) claims more than is there.
    let len = len.min(file.metadata()?.len().saturating_sub(start));
    Ok(Some(Window { file, start, len, pos: 0 }))
}

/// Byte offset and length of the `data` chunk, if the `fmt ` chunk before it
/// names MPEG audio.
fn find_mpeg_data(r: &mut (impl Read + Seek)) -> io::Result<Option<(u64, u64)>> {
    let mut header = [0u8; 12];
    if r.read_exact(&mut header).is_err() || &header[..4] != b"RIFF" || &header[8..] != b"WAVE" {
        return Ok(None);
    }
    let mut mpeg = false;
    loop {
        let mut chunk = [0u8; 8];
        if r.read_exact(&mut chunk).is_err() {
            return Ok(None);
        }
        let len = u64::from(u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]));
        match &chunk[..4] {
            b"fmt " => {
                let mut fmt = vec![0u8; len.min(64) as usize];
                r.read_exact(&mut fmt)?;
                let tag = |at: usize| fmt.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
                // Extensible: the sub-format GUID begins with the tag.
                let format = match tag(0) {
                    Some(WAVE_FORMAT_EXTENSIBLE) => tag(24),
                    other => other,
                };
                mpeg = matches!(format, Some(WAVE_FORMAT_MPEG | WAVE_FORMAT_MPEGLAYER3));
                if !mpeg {
                    return Ok(None);
                }
                r.seek(SeekFrom::Current((len + (len & 1)) as i64 - fmt.len() as i64))?;
            }
            b"data" if mpeg => return Ok(Some((r.stream_position()?, len))),
            // Data before its format: not a file to guess about.
            b"data" => return Ok(None),
            // Chunks are padded to an even length.
            _ => {
                r.seek(SeekFrom::Current((len + (len & 1)) as i64))?;
            }
        }
    }
}

/// A stretch of a file, read and sought as if it were the whole file.
pub(crate) struct Window {
    file: File,
    start: u64,
    len: u64,
    pos: u64,
}

impl Read for Window {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let left = self.len.saturating_sub(self.pos);
        let n = (buf.len() as u64).min(left) as usize;
        if n == 0 {
            return Ok(0);
        }
        self.file.seek(SeekFrom::Start(self.start + self.pos))?;
        let n = self.file.read(&mut buf[..n])?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for Window {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let pos = match to {
            SeekFrom::Start(p) => Some(p),
            SeekFrom::End(d) => self.len.checked_add_signed(d),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
        };
        let pos = pos.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before the start"))?;
        self.pos = pos;
        Ok(pos)
    }
}

impl MediaSource for Window {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        Some(self.len)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn wav(chunks: &[(&[u8; 4], Vec<u8>)]) -> Cursor<Vec<u8>> {
        let mut b = b"RIFF\0\0\0\0WAVE".to_vec();
        for (id, data) in chunks {
            b.extend_from_slice(*id);
            b.extend_from_slice(&(data.len() as u32).to_le_bytes());
            b.extend_from_slice(data);
            if data.len() % 2 == 1 {
                b.push(0);
            }
        }
        Cursor::new(b)
    }

    fn fmt(tag: u16) -> Vec<u8> {
        let mut f = tag.to_le_bytes().to_vec();
        f.resize(30, 0);
        f
    }

    #[test]
    fn finds_the_data_of_mpeg_formats_only() {
        let audio = vec![0xff, 0xfb, 1, 2, 3];
        let at = |chunks: &[(&[u8; 4], Vec<u8>)]| find_mpeg_data(&mut wav(chunks)).unwrap();
        // Header 12, fmt 8 + 30, odd LIST chunk 8 + 3 + pad, data header 8.
        let list = (b"LIST", vec![1, 2, 3]);
        assert_eq!(at(&[(b"fmt ", fmt(0x0055)), list.clone(), (b"data", audio.clone())]), Some((70, 5)));
        assert_eq!(at(&[(b"fmt ", fmt(0x0050)), (b"data", audio.clone())]), Some((58, 5)), "MP2");
        let mut ext = fmt(WAVE_FORMAT_EXTENSIBLE);
        ext[24..26].copy_from_slice(&0x0055u16.to_le_bytes());
        assert_eq!(at(&[(b"fmt ", ext), (b"data", audio.clone())]), Some((58, 5)));

        assert_eq!(at(&[(b"fmt ", fmt(0x0001)), (b"data", audio.clone())]), None, "PCM is symphonia's");
        assert_eq!(at(&[(b"data", audio.clone()), (b"fmt ", fmt(0x0055))]), None);
        assert_eq!(at(&[(b"fmt ", fmt(0x0055))]), None, "no data");
        assert_eq!(find_mpeg_data(&mut Cursor::new(b"ID3\x04".to_vec())).unwrap(), None);
    }

    #[test]
    fn a_window_reads_and_seeks_inside_its_stretch() {
        let path = std::env::temp_dir().join(format!("jq-media-window-{}", std::process::id()));
        std::fs::write(&path, b"0123456789").unwrap();
        let mut w = Window { file: File::open(&path).unwrap(), start: 3, len: 4, pos: 0 };
        let mut all = Vec::new();
        w.read_to_end(&mut all).unwrap();
        assert_eq!(all, b"3456");
        assert_eq!(w.seek(SeekFrom::End(-1)).unwrap(), 3);
        let mut one = [0u8; 4];
        assert_eq!(w.read(&mut one).unwrap(), 1);
        assert_eq!(one[0], b'6');
        assert_eq!(w.seek(SeekFrom::Current(-3)).unwrap(), 1);
        assert!(w.seek(SeekFrom::Current(-2)).is_err());
        assert_eq!((w.is_seekable(), w.byte_len()), (true, Some(4)));
        std::fs::remove_file(path).ok();
    }
}
