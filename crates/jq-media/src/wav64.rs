//! WAV files beyond RIFF's 4 GB: RF64 and BW64 (EBU Tech 3306, ITU-R
//! BS.2088; field recorders and broadcast software write them, some always)
//! and Sony Wave64 (`.w64`, Vegas and Sound Forge). symphonia only reads
//! RIFF, so these are read here: PCM or float samples, the only contents
//! these files carry in practice, handed to symphonia's PCM decoder in
//! packets.
//!
//! Only used after symphonia has refused a file (see [`crate::open_format`]).

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use symphonia::core::audio::Channels;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::codecs::audio::AudioCodecId;
use symphonia::core::codecs::audio::AudioCodecParameters;
use symphonia::core::codecs::audio::well_known::{
    CODEC_ID_PCM_F32LE, CODEC_ID_PCM_F64LE, CODEC_ID_PCM_S16LE, CODEC_ID_PCM_S24LE, CODEC_ID_PCM_S32LE,
    CODEC_ID_PCM_U8,
};
use symphonia::core::errors::{Error, Result, SeekErrorKind};
use symphonia::core::formats::prelude::*;
use symphonia::core::formats::well_known::FORMAT_ID_WAVE;
use symphonia::core::io::{MediaSourceStream, ReadBytes};
use symphonia::core::meta::{Metadata, MetadataLog};
use symphonia::core::units::{Duration, TimeBase, Timestamp};

const WAVE_FORMAT_PCM: u16 = 0x0001;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 0x0003;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xfffe;

/// The tail every Wave64 GUID but the outer `riff` one shares after its
/// four-character code.
const W64_GUID_TAIL: [u8; 12] = [0xf3, 0xac, 0xd3, 0x11, 0x8c, 0xd1, 0x00, 0xc0, 0x4f, 0x8e, 0xdb, 0x8a];
const W64_RIFF: [u8; 16] =
    [b'r', b'i', b'f', b'f', 0x2e, 0x91, 0xcf, 0x11, 0xa5, 0xd6, 0x28, 0xdb, 0x04, 0xc1, 0x00, 0x00];

/// Frames per packet handed to the decoder.
const PACKET_FRAMES: u64 = 4096;

const INFO: FormatInfo =
    FormatInfo { format: FORMAT_ID_WAVE, short_name: "wave64", long_name: "RF64/BW64/Wave64" };

/// How the samples are stored.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Pcm {
    codec: AudioCodecId,
    channels: u16,
    rate: u32,
    /// Bytes per frame.
    block: u16,
    /// Bits of each sample that carry signal (24 in a 32-bit container).
    valid_bits: Option<u16>,
}

/// Where the samples are, once the container is understood.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Layout {
    pcm: Pcm,
    data_start: u64,
    data_len: u64,
}

/// A reader for `path` if it is an RF64, BW64 or Wave64 file holding PCM or
/// float samples. `None` for any other file.
pub(crate) fn open(path: &Path) -> io::Result<Option<Wav64Reader>> {
    let mut file = File::open(path)?;
    let Some(layout) = find_layout(&mut file)? else { return Ok(None) };
    // A file cut short (or written while recording) claims more than is there.
    let data_len = layout.data_len.min(file.metadata()?.len().saturating_sub(layout.data_start));
    let block = u64::from(layout.pcm.block);
    let frames = data_len / block;
    let mut mss = MediaSourceStream::new(Box::new(file), Default::default());
    mss.seek(SeekFrom::Start(layout.data_start))?;
    let Pcm { codec, channels, rate, valid_bits, .. } = layout.pcm;
    let mut params = AudioCodecParameters::new();
    params
        .for_codec(codec)
        .with_sample_rate(rate)
        .with_channels(Channels::Discrete(channels))
        .with_max_frames_per_packet(PACKET_FRAMES)
        .with_frames_per_block(1);
    if let Some(bits) = valid_bits {
        params.with_bits_per_sample(u32::from(bits)).with_bits_per_coded_sample(u32::from(bits));
    }
    let mut track = Track::new(0);
    track
        .with_codec_params(CodecParameters::Audio(params))
        .with_num_frames(frames)
        .with_duration(Duration::from(frames));
    // `read_fmt` refuses a rate of 0.
    if let Some(tb) = TimeBase::try_from_recip(rate) {
        track.with_time_base(tb);
    }
    Ok(Some(Wav64Reader {
        media_info: MediaInfo::from_track(&track),
        tracks: vec![track],
        mss,
        metadata: MetadataLog::default(),
        data_start: layout.data_start,
        data_end: layout.data_start + frames * block,
        block,
        frames,
    }))
}

fn find_layout(r: &mut (impl Read + Seek)) -> io::Result<Option<Layout>> {
    let mut head = [0u8; 16];
    if r.read_exact(&mut head[..12]).is_err() {
        return Ok(None);
    }
    if matches!(&head[..4], b"RF64" | b"BW64") && &head[8..12] == b"WAVE" {
        find_rf64(r)
    } else if r.read_exact(&mut head[12..]).is_ok() && head == W64_RIFF {
        find_w64(r)
    } else {
        Ok(None)
    }
}

/// RF64/BW64: RIFF chunks, whose 32-bit sizes are 0xFFFFFFFF where the real
/// one is too big; the `ds64` chunk right after the header has it.
fn find_rf64(r: &mut (impl Read + Seek)) -> io::Result<Option<Layout>> {
    let mut ds64_data_len = None;
    let mut pcm = None;
    loop {
        let mut chunk = [0u8; 8];
        if r.read_exact(&mut chunk).is_err() {
            return Ok(None);
        }
        let len = u64::from(u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]));
        let next = r.stream_position()? + len + (len & 1);
        match &chunk[..4] {
            b"ds64" => {
                // RIFF size, data size, sample count (each 64 bits), table.
                let mut ds64 = [0u8; 16];
                r.read_exact(&mut ds64)?;
                ds64_data_len = Some(u64::from_le_bytes(ds64[8..].try_into().unwrap_or_default()));
            }
            b"fmt " => pcm = read_fmt(r, len)?,
            b"data" => {
                let data_len = match (len, ds64_data_len) {
                    (0xffff_ffff, Some(real)) => real,
                    (0xffff_ffff, None) => return Ok(None),
                    (len, _) => len,
                };
                return Ok(pcm.map(|pcm| Layout {
                    pcm,
                    data_start: r.stream_position().unwrap_or(0),
                    data_len,
                }));
            }
            _ => {}
        }
        r.seek(SeekFrom::Start(next))?;
    }
}

/// Wave64: chunks named by GUID, with 64-bit sizes that count their own
/// 24-byte header, aligned to 8 bytes.
fn find_w64(r: &mut (impl Read + Seek)) -> io::Result<Option<Layout>> {
    let mut riff = [0u8; 24];
    r.read_exact(&mut riff[..8])?; // size of the whole file
    r.read_exact(&mut riff[8..])?;
    if &riff[8..12] != b"wave" || riff[12..] != W64_GUID_TAIL {
        return Ok(None);
    }
    let mut pcm = None;
    loop {
        let mut chunk = [0u8; 24];
        if r.read_exact(&mut chunk).is_err() {
            return Ok(None);
        }
        let size = u64::from_le_bytes(chunk[16..].try_into().unwrap_or_default());
        let Some(len) = size.checked_sub(24) else { return Ok(None) };
        let start = r.stream_position()?;
        if chunk[4..16] != W64_GUID_TAIL {
            // Not one of the standard chunks (a marker or summary list).
        } else if &chunk[..4] == b"fmt " {
            pcm = read_fmt(r, len)?;
        } else if &chunk[..4] == b"data" {
            return Ok(pcm.map(|pcm| Layout { pcm, data_start: start, data_len: len }));
        }
        r.seek(SeekFrom::Start(start + len.next_multiple_of(8)))?;
    }
}

/// The sample format of a `fmt ` chunk of `len` bytes; `None` for anything
/// but PCM or float.
fn read_fmt(r: &mut impl Read, len: u64) -> io::Result<Option<Pcm>> {
    let mut fmt = [0u8; 40];
    let n = (len as usize).min(fmt.len());
    if n < 16 {
        return Ok(None);
    }
    r.read_exact(&mut fmt[..n])?;
    let u16_at = |at: usize| u16::from_le_bytes([fmt[at], fmt[at + 1]]);
    let (tag, channels, block, bits) = (u16_at(0), u16_at(2), u16_at(12), u16_at(14));
    let rate = u32::from_le_bytes([fmt[4], fmt[5], fmt[6], fmt[7]]);
    // Extensible: the sub-format GUID begins with the tag, and the valid
    // bits may be fewer than the container's.
    let (tag, valid_bits) = match tag {
        WAVE_FORMAT_EXTENSIBLE if n >= 26 => (u16_at(24), Some(u16_at(18)).filter(|&v| v != 0 && v < bits)),
        WAVE_FORMAT_EXTENSIBLE => return Ok(None),
        tag => (tag, None),
    };
    let codec = match (tag, bits) {
        (WAVE_FORMAT_PCM, 8) => CODEC_ID_PCM_U8,
        (WAVE_FORMAT_PCM, 16) => CODEC_ID_PCM_S16LE,
        (WAVE_FORMAT_PCM, 24) => CODEC_ID_PCM_S24LE,
        (WAVE_FORMAT_PCM, 32) => CODEC_ID_PCM_S32LE,
        (WAVE_FORMAT_IEEE_FLOAT, 32) => CODEC_ID_PCM_F32LE,
        (WAVE_FORMAT_IEEE_FLOAT, 64) => CODEC_ID_PCM_F64LE,
        _ => return Ok(None),
    };
    // Frames are whole samples of every channel, nothing in between.
    if channels == 0 || rate == 0 || u32::from(block) != u32::from(channels) * u32::from(bits / 8) {
        return Ok(None);
    }
    let valid_bits = valid_bits.filter(|_| tag == WAVE_FORMAT_PCM);
    Ok(Some(Pcm { codec, channels, rate, block, valid_bits }))
}

/// Packets of up to [`PACKET_FRAMES`] frames from the samples of an RF64,
/// BW64 or Wave64 file.
pub(crate) struct Wav64Reader {
    mss: MediaSourceStream<'static>,
    tracks: Vec<Track>,
    media_info: MediaInfo,
    metadata: MetadataLog,
    data_start: u64,
    data_end: u64,
    block: u64,
    frames: u64,
}

impl FormatReader for Wav64Reader {
    fn format_info(&self) -> &FormatInfo {
        &INFO
    }

    fn media_info(&self) -> &MediaInfo {
        &self.media_info
    }

    fn metadata(&mut self) -> Metadata<'_> {
        self.metadata.metadata()
    }

    fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    fn next_packet(&mut self) -> Result<Option<Packet>> {
        let pos = self.mss.pos();
        let frames = (self.data_end.saturating_sub(pos) / self.block).min(PACKET_FRAMES);
        if frames == 0 || pos < self.data_start {
            return Ok(None);
        }
        let pts = Timestamp::new(((pos - self.data_start) / self.block) as i64);
        let data = self.mss.read_boxed_slice((frames * self.block) as usize)?;
        Ok(Some(Packet::new(0, pts, Duration::from(frames), data)))
    }

    fn seek(&mut self, _mode: SeekMode, to: SeekTo) -> Result<SeekedTo> {
        let required_ts = match to {
            SeekTo::Timestamp { ts, .. } => ts,
            SeekTo::Time { time, .. } => {
                let tb = self.tracks[0].time_base.ok_or(Error::SeekError(SeekErrorKind::Unseekable))?;
                tb.calc_timestamp(time).ok_or(Error::SeekError(SeekErrorKind::OutOfRange))?
            }
        };
        let frame =
            u64::try_from(required_ts.get()).map_err(|_| Error::SeekError(SeekErrorKind::OutOfRange))?;
        if frame > self.frames {
            return Err(Error::SeekError(SeekErrorKind::OutOfRange));
        }
        self.mss.seek(SeekFrom::Start(self.data_start + frame * self.block))?;
        Ok(SeekedTo { track_id: 0, required_ts, actual_ts: required_ts })
    }

    fn into_inner<'s>(self: Box<Self>) -> MediaSourceStream<'s>
    where
        Self: 's,
    {
        self.mss
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn fmt(tag: u16, channels: u16, bits: u16) -> Vec<u8> {
        let mut f = Vec::new();
        f.extend_from_slice(&tag.to_le_bytes());
        f.extend_from_slice(&channels.to_le_bytes());
        f.extend_from_slice(&48_000u32.to_le_bytes());
        f.extend_from_slice(&(48_000 * u32::from(channels * bits / 8)).to_le_bytes());
        f.extend_from_slice(&(channels * bits / 8).to_le_bytes());
        f.extend_from_slice(&bits.to_le_bytes());
        f
    }

    fn extensible(sub: u16, channels: u16, bits: u16, valid: u16) -> Vec<u8> {
        let mut f = fmt(WAVE_FORMAT_EXTENSIBLE, channels, bits);
        f.extend_from_slice(&22u16.to_le_bytes());
        f.extend_from_slice(&valid.to_le_bytes());
        f.extend_from_slice(&3u32.to_le_bytes());
        f.extend_from_slice(&sub.to_le_bytes());
        f.extend_from_slice(&[0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71]);
        f
    }

    fn rf64(chunks: &[(&[u8; 4], u32, Vec<u8>)]) -> Cursor<Vec<u8>> {
        let mut b = b"RF64\xff\xff\xff\xffWAVE".to_vec();
        for (id, len, data) in chunks {
            b.extend_from_slice(*id);
            b.extend_from_slice(&len.to_le_bytes());
            b.extend_from_slice(data);
            if data.len() % 2 == 1 {
                b.push(0);
            }
        }
        Cursor::new(b)
    }

    fn ds64(data_len: u64) -> Vec<u8> {
        let mut d = vec![0u8; 28];
        d[8..16].copy_from_slice(&data_len.to_le_bytes());
        d
    }

    fn w64(chunks: &[(&[u8; 4], Vec<u8>)]) -> Cursor<Vec<u8>> {
        let mut b = W64_RIFF.to_vec();
        b.extend_from_slice(&0u64.to_le_bytes());
        b.extend_from_slice(b"wave");
        b.extend_from_slice(&W64_GUID_TAIL);
        for (id, data) in chunks {
            b.extend_from_slice(*id);
            b.extend_from_slice(&W64_GUID_TAIL);
            b.extend_from_slice(&(data.len() as u64 + 24).to_le_bytes());
            b.extend_from_slice(data);
            b.resize(b.len().next_multiple_of(8), 0);
        }
        Cursor::new(b)
    }

    const S24: Pcm = Pcm { codec: CODEC_ID_PCM_S24LE, channels: 2, rate: 48_000, block: 6, valid_bits: None };

    #[test]
    fn rf64_takes_the_data_size_from_ds64() {
        let big = 5u64 << 30; // more than RIFF's 4 GB
        let data = vec![0u8; 12];
        let mut file = rf64(&[
            (b"ds64", 28, ds64(big)),
            (b"fmt ", 16, fmt(WAVE_FORMAT_PCM, 2, 24)),
            (b"LIST", 3, vec![1, 2, 3]),
            (b"data", 0xffff_ffff, data.clone()),
        ]);
        // Header 12, ds64 8 + 28, fmt 8 + 16, odd LIST 8 + 3 + pad, data header 8.
        let layout = find_layout(&mut file).unwrap().unwrap();
        assert_eq!(layout, Layout { pcm: S24, data_start: 92, data_len: big });

        // A small RF64 file states the size in the chunk itself.
        let mut small = rf64(&[(b"ds64", 28, ds64(0)), (b"fmt ", 16, fmt(1, 2, 24)), (b"data", 12, data)]);
        assert_eq!(find_layout(&mut small).unwrap().unwrap().data_len, 12);
        let mut bw64 = rf64(&[(b"fmt ", 16, fmt(3, 1, 32)), (b"data", 4, vec![0; 4])]);
        bw64.get_mut()[..4].copy_from_slice(b"BW64");
        assert_eq!(find_layout(&mut bw64).unwrap().unwrap().pcm.codec, CODEC_ID_PCM_F32LE);
    }

    #[test]
    fn wave64_finds_its_chunks_by_guid() {
        let mut file =
            w64(&[(b"fmt ", fmt(WAVE_FORMAT_PCM, 2, 24)), (b"levl", vec![1; 5]), (b"data", vec![0; 12])]);
        // riff 40, fmt 24 + 16, levl 24 + 5 + pad to 8, data header 24.
        let layout = find_layout(&mut file).unwrap().unwrap();
        assert_eq!(layout, Layout { pcm: S24, data_start: 136, data_len: 12 });
    }

    #[test]
    fn only_pcm_and_float_are_read() {
        let pcm = |f: Vec<u8>| {
            let mut file = rf64(&[(b"fmt ", f.len() as u32, f), (b"data", 0, vec![])]);
            find_layout(&mut file).unwrap().map(|l| l.pcm)
        };
        assert_eq!(pcm(fmt(1, 1, 8)).unwrap().codec, CODEC_ID_PCM_U8);
        assert_eq!(pcm(fmt(1, 6, 16)).unwrap().codec, CODEC_ID_PCM_S16LE);
        assert_eq!(pcm(fmt(3, 2, 64)).unwrap().codec, CODEC_ID_PCM_F64LE);
        let in32 = pcm(extensible(WAVE_FORMAT_PCM, 2, 32, 24)).unwrap();
        assert_eq!((in32.codec, in32.block, in32.valid_bits), (CODEC_ID_PCM_S32LE, 8, Some(24)));
        assert_eq!(pcm(extensible(WAVE_FORMAT_IEEE_FLOAT, 2, 32, 32)).unwrap().codec, CODEC_ID_PCM_F32LE);

        assert_eq!(pcm(fmt(0x0002, 2, 4)), None, "ADPCM");
        assert_eq!(pcm(fmt(0x0055, 2, 0)), None, "MP3");
        assert_eq!(pcm(fmt(1, 2, 20)), None, "20-bit samples");
        assert_eq!(pcm(fmt(1, 0, 16)), None, "no channels");
        let mut odd_block = fmt(1, 2, 16);
        odd_block[12] = 5;
        assert_eq!(pcm(odd_block), None);
    }

    #[test]
    fn other_files_are_left_alone() {
        let layout = |b: &[u8]| find_layout(&mut Cursor::new(b.to_vec())).unwrap();
        assert_eq!(layout(b"RIFF\0\0\0\0WAVEfmt "), None);
        assert_eq!(layout(b"RF64"), None, "too short");
        // RF64 whose data size is in a ds64 chunk that is missing.
        let mut no_ds64 = rf64(&[(b"fmt ", 16, fmt(1, 2, 16)), (b"data", 0xffff_ffff, vec![0; 4])]);
        assert_eq!(find_layout(&mut no_ds64).unwrap(), None);
        // Data before its format.
        let mut data_first = rf64(&[(b"data", 4, vec![0; 4]), (b"fmt ", 16, fmt(1, 2, 16))]);
        assert_eq!(find_layout(&mut data_first).unwrap(), None);
        let mut w64_no_data = w64(&[(b"fmt ", fmt(1, 2, 16))]);
        assert_eq!(find_layout(&mut w64_no_data).unwrap(), None);
    }
}
