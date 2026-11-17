//! Opus, decoded by libopus (through `opusic-sys`): symphonia reads Opus
//! from Ogg (and other containers) but has no decoder for it.
//!
//! The container takes care of the pre-skip: symphonia's Ogg reader marks
//! those frames for trimming in the packets, as for any codec with an
//! encoder delay. What the identification header (`OpusHead`) adds is read
//! here: the output gain, applied by libopus, and for surround the streams
//! and the channel mapping, reordered so that the decoded channels come out
//! in symphonia's order (front left and right first).

use std::ffi::c_int;
use std::ptr::NonNull;

use opusic_sys as ffi;
use symphonia::core::audio::{
    AsGenericAudioBufferRef, AudioBuffer, AudioMut, AudioSpec, Channels, GenericAudioBufferRef, Position,
};
use symphonia::core::codecs::CodecInfo;
use symphonia::core::codecs::audio::well_known::CODEC_ID_OPUS;
use symphonia::core::codecs::audio::{
    AudioCodecParameters, AudioDecoder, AudioDecoderOptions, FinalizeResult,
};
use symphonia::core::codecs::registry::{RegisterableAudioDecoder, SupportedAudioCodec};
use symphonia::core::errors::{Error, Result, decode_error, unsupported_error};
use symphonia::core::packet::PacketRef;

/// Opus always decodes at 48 kHz here (it may run at less, but never needs to).
const RATE: u32 = 48_000;
/// The longest packet: 120 ms.
const MAX_FRAMES: usize = RATE as usize * 120 / 1000;

/// What the decoder needs from the identification header.
#[derive(Debug, PartialEq)]
struct Head {
    channels: u8,
    /// Q7.8 dB.
    gain: i16,
    streams: u8,
    coupled: u8,
    /// Decoded channel of each output channel, in the order of the header.
    mapping: Vec<u8>,
}

impl Head {
    /// Parses an `OpusHead` (RFC 7845, section 5.1).
    fn parse(b: &[u8]) -> Option<Self> {
        if b.len() < 19 || &b[..8] != b"OpusHead" || b[8] >> 4 != 0 || b[9] == 0 {
            return None;
        }
        let (channels, gain) = (b[9], i16::from_le_bytes([b[16], b[17]]));
        match b[18] {
            // Mono or stereo, one stream.
            0 if channels <= 2 => Some(Self {
                channels,
                gain,
                streams: 1,
                coupled: channels - 1,
                mapping: (0..channels).collect(),
            }),
            // Vorbis channel order, up to 7.1.
            1 if channels <= 8 => {
                let (streams, coupled) = (*b.get(19)?, *b.get(20)?);
                let mapping = b.get(21..21 + usize::from(channels))?.to_vec();
                let decoded = u16::from(streams) + u16::from(coupled);
                let valid = streams > 0
                    && coupled <= streams
                    && mapping.iter().all(|&m| m == 255 || u16::from(m) < decoded);
                valid.then_some(Self { channels, gain, streams, coupled, mapping })
            }
            _ => None,
        }
    }

    /// The speaker positions of the channels, in the order of the header.
    fn positions(&self) -> Vec<Position> {
        use Position as P;
        match self.channels {
            1 => vec![P::FRONT_LEFT],
            2 => vec![P::FRONT_LEFT, P::FRONT_RIGHT],
            3 => vec![P::FRONT_LEFT, P::FRONT_CENTER, P::FRONT_RIGHT],
            4 => vec![P::FRONT_LEFT, P::FRONT_RIGHT, P::REAR_LEFT, P::REAR_RIGHT],
            5 => vec![P::FRONT_LEFT, P::FRONT_CENTER, P::FRONT_RIGHT, P::REAR_LEFT, P::REAR_RIGHT],
            6 => vec![P::FRONT_LEFT, P::FRONT_CENTER, P::FRONT_RIGHT, P::REAR_LEFT, P::REAR_RIGHT, P::LFE1],
            7 => vec![
                P::FRONT_LEFT,
                P::FRONT_CENTER,
                P::FRONT_RIGHT,
                P::SIDE_LEFT,
                P::SIDE_RIGHT,
                P::REAR_CENTER,
                P::LFE1,
            ],
            _ => vec![
                P::FRONT_LEFT,
                P::FRONT_CENTER,
                P::FRONT_RIGHT,
                P::SIDE_LEFT,
                P::SIDE_RIGHT,
                P::REAR_LEFT,
                P::REAR_RIGHT,
                P::LFE1,
            ],
        }
    }

    /// The channel layout and the mapping that makes libopus put out the
    /// channels in its order: symphonia keeps the channels of a buffer
    /// ordered by position.
    fn layout(&self) -> (Channels, Vec<u8>) {
        let positions = self.positions();
        let mut order: Vec<usize> = (0..positions.len()).collect();
        order.sort_by_key(|&i| positions[i].bits());
        let all = positions.iter().fold(Position::empty(), |all, &p| all | p);
        (Channels::Positioned(all), order.iter().map(|&i| self.mapping[i]).collect())
    }
}

/// A libopus multistream decoder.
struct Libopus(NonNull<ffi::OpusMSDecoder>);

// The decoder is plain memory, used by one thread at a time.
unsafe impl Send for Libopus {}
unsafe impl Sync for Libopus {}

impl Drop for Libopus {
    fn drop(&mut self) {
        unsafe { ffi::opus_multistream_decoder_destroy(self.0.as_ptr()) }
    }
}

/// [`AudioDecoder`] for Opus, registered in [`crate::codecs`].
pub(crate) struct OpusDecoder {
    params: AudioCodecParameters,
    gapless: bool,
    decoder: Libopus,
    channels: usize,
    /// Interleaved output of libopus, room for the longest packet.
    pcm: Vec<f32>,
    buf: AudioBuffer<f32>,
}

impl OpusDecoder {
    fn try_new(params: &AudioCodecParameters, opts: &AudioDecoderOptions) -> Result<Self> {
        if params.codec != CODEC_ID_OPUS {
            return unsupported_error("opus: invalid codec");
        }
        // Without a header (not from Ogg): mono or stereo, as the container says.
        let head = match params.extra_data.as_deref() {
            Some(extra) => Head::parse(extra),
            None => {
                params.channels.as_ref().map(Channels::count).filter(|&n| (1..=2).contains(&n)).map(|n| {
                    Head {
                        channels: n as u8,
                        gain: 0,
                        streams: 1,
                        coupled: n as u8 - 1,
                        mapping: (0..n as u8).collect(),
                    }
                })
            }
        };
        let Some(head) = head else { return unsupported_error("opus: unsupported channel mapping") };
        let (layout, mapping) = head.layout();
        let mut error = 0;
        let decoder = unsafe {
            ffi::opus_multistream_decoder_create(
                RATE as i32,
                c_int::from(head.channels),
                c_int::from(head.streams),
                c_int::from(head.coupled),
                mapping.as_ptr(),
                &mut error,
            )
        };
        let decoder = match NonNull::new(decoder) {
            Some(d) if error == ffi::OPUS_OK => Libopus(d),
            _ => return decode_error("opus: invalid stream layout"),
        };
        if head.gain != 0 {
            let set = unsafe {
                ffi::opus_multistream_decoder_ctl(
                    decoder.0.as_ptr(),
                    ffi::OPUS_SET_GAIN_REQUEST,
                    c_int::from(head.gain),
                )
            };
            if set != ffi::OPUS_OK {
                return decode_error("opus: invalid output gain");
            }
        }
        let channels = usize::from(head.channels);
        let mut params = params.clone();
        params.with_sample_rate(RATE).with_channels(layout.clone());
        Ok(Self {
            params,
            gapless: opts.gapless,
            decoder,
            channels,
            pcm: vec![0.0; MAX_FRAMES * channels],
            buf: AudioBuffer::new(AudioSpec::new(RATE, layout), MAX_FRAMES),
        })
    }
}

impl AudioDecoder for OpusDecoder {
    fn reset(&mut self) {
        unsafe { ffi::opus_multistream_decoder_ctl(self.decoder.0.as_ptr(), ffi::OPUS_RESET_STATE) };
    }

    fn codec_info(&self) -> &CodecInfo {
        &Self::supported_codecs()[0].info
    }

    fn codec_params(&self) -> &AudioCodecParameters {
        &self.params
    }

    fn decode_ref(&mut self, packet: &PacketRef<'_>) -> Result<GenericAudioBufferRef<'_>> {
        self.buf.clear();
        let Ok(len) = i32::try_from(packet.data.len()) else { return decode_error("opus: packet too long") };
        // An empty packet is a lost one: libopus conceals it.
        let data = if packet.data.is_empty() { std::ptr::null() } else { packet.data.as_ptr() };
        let frames = unsafe {
            ffi::opus_multistream_decode_float(
                self.decoder.0.as_ptr(),
                data,
                len,
                self.pcm.as_mut_ptr(),
                MAX_FRAMES as c_int,
                0,
            )
        };
        let Ok(frames) = usize::try_from(frames) else {
            return Err(Error::DecodeError("opus: corrupt packet"));
        };
        self.buf.render_uninit(Some(frames));
        self.buf.copy_from_slice_interleaved(&&self.pcm[..frames * self.channels]);
        if self.gapless {
            self.buf.trim(packet.trim_start.get() as usize, packet.trim_end.get() as usize);
        }
        Ok(self.buf.as_generic_audio_buffer_ref())
    }

    fn finalize(&mut self) -> FinalizeResult {
        FinalizeResult::default()
    }

    fn last_decoded(&self) -> GenericAudioBufferRef<'_> {
        self.buf.as_generic_audio_buffer_ref()
    }
}

impl RegisterableAudioDecoder for OpusDecoder {
    fn try_registry_new(
        params: &AudioCodecParameters,
        opts: &AudioDecoderOptions,
    ) -> Result<Box<dyn AudioDecoder>> {
        Ok(Box::new(Self::try_new(params, opts)?))
    }

    fn supported_codecs() -> &'static [SupportedAudioCodec] {
        // What symphonia's `support_audio_codec!` writes, which needs the
        // `symphonia_core` crate by name.
        const OPUS: [SupportedAudioCodec; 1] = [SupportedAudioCodec {
            id: CODEC_ID_OPUS,
            info: CodecInfo { short_name: "opus", long_name: "Opus", profiles: &[] },
        }];
        &OPUS
    }
}

#[cfg(test)]
mod tests {
    use symphonia::core::units::{Duration, Timestamp};

    use super::*;

    fn head(channels: u8, gain: i16, family: u8, tail: &[u8]) -> Vec<u8> {
        let mut b = b"OpusHead\x01".to_vec();
        b.push(channels);
        b.extend_from_slice(&312u16.to_le_bytes());
        b.extend_from_slice(&44_100u32.to_le_bytes());
        b.extend_from_slice(&gain.to_le_bytes());
        b.push(family);
        b.extend_from_slice(tail);
        b
    }

    #[test]
    fn reads_the_identification_header() {
        let stereo = Head::parse(&head(2, -256, 0, &[])).unwrap();
        assert_eq!(stereo, Head { channels: 2, gain: -256, streams: 1, coupled: 1, mapping: vec![0, 1] });
        assert_eq!(Head::parse(&head(1, 0, 0, &[])).unwrap().mapping, [0]);
        // 5.1 as libopus encodes it: L+R and RL+RR coupled, then C and LFE.
        let surround = Head::parse(&head(6, 0, 1, &[4, 2, 0, 4, 1, 2, 3, 5])).unwrap();
        assert_eq!((surround.streams, surround.coupled), (4, 2));

        assert_eq!(Head::parse(&head(3, 0, 0, &[])), None, "family 0 is mono or stereo");
        assert_eq!(Head::parse(&head(6, 0, 1, &[4, 2, 0, 4])), None, "mapping cut short");
        assert_eq!(Head::parse(&head(2, 0, 1, &[1, 1, 0, 2])), None, "channel 2 of 2 decoded");
        assert_eq!(Head::parse(&head(4, 0, 255, &[4, 0, 0, 1, 2, 3])), None, "ambisonics etc.");
        assert_eq!(Head::parse(&head(0, 0, 0, &[])), None);
        assert_eq!(Head::parse(b"OpusTags\x01\x02"), None);
    }

    /// `count` packets of 20 ms of a mono sine at 0.25, encoded by libopus.
    fn encoded_sine(count: usize) -> Vec<Vec<u8>> {
        unsafe {
            let mut error = 0;
            let encoder = ffi::opus_encoder_create(RATE as i32, 1, ffi::OPUS_APPLICATION_AUDIO, &mut error);
            assert_eq!(error, ffi::OPUS_OK);
            let packets = (0..count)
                .map(|p| {
                    let pcm: Vec<f32> = (p * 960..(p + 1) * 960)
                        .map(|i| 0.25 * (std::f32::consts::TAU * 441.0 * i as f32 / RATE as f32).sin())
                        .collect();
                    let mut out = vec![0u8; 1500];
                    let n = ffi::opus_encode_float(encoder, pcm.as_ptr(), 960, out.as_mut_ptr(), 1500);
                    out.truncate(usize::try_from(n).unwrap());
                    out
                })
                .collect();
            ffi::opus_encoder_destroy(encoder);
            packets
        }
    }

    /// Peak of the last of `packets` decoded with an output gain of `gain` (Q7.8 dB).
    fn last_peak(packets: &[Vec<u8>], gain: i16) -> f32 {
        let mut params = AudioCodecParameters::new();
        params.for_codec(CODEC_ID_OPUS).with_extra_data(head(1, gain, 0, &[]).into_boxed_slice());
        let mut decoder = OpusDecoder::try_new(&params, &AudioDecoderOptions::default()).unwrap();
        for (i, data) in packets.iter().enumerate() {
            let packet = PacketRef::new(0, Timestamp::new(i as i64 * 960), Duration::from(960u64), data);
            assert_eq!(decoder.decode_ref(&packet).unwrap().frames(), 960);
        }
        decoder.pcm[..960].iter().fold(0.0, |m, s| m.max(s.abs()))
    }

    #[test]
    fn applies_the_output_gain() {
        let packets = encoded_sine(10);
        let plain = last_peak(&packets, 0);
        assert!((plain - 0.25).abs() < 0.02, "{plain}");
        // −6.02 dB halves.
        let halved = last_peak(&packets, -1541);
        assert!((halved / plain - 0.5).abs() < 0.01, "{halved} of {plain}");
    }

    #[test]
    fn a_corrupt_packet_is_an_error_and_a_lost_one_is_concealed() {
        let mut params = AudioCodecParameters::new();
        params.for_codec(CODEC_ID_OPUS).with_extra_data(head(2, 0, 0, &[]).into_boxed_slice());
        let mut decoder = OpusDecoder::try_new(&params, &AudioDecoderOptions::default()).unwrap();
        let packet = |data| PacketRef::new(0, Timestamp::new(0), Duration::from(960u64), data);
        // A code 3 packet (TOC 0x03) without its frame count byte.
        assert!(decoder.decode_ref(&packet(&[0x03])).is_err());
        assert!(decoder.decode_ref(&packet(&[])).unwrap().frames() > 0);

        let mut vorbis = params.clone();
        vorbis.for_codec(symphonia::core::codecs::audio::well_known::CODEC_ID_VORBIS);
        assert!(OpusDecoder::try_new(&vorbis, &AudioDecoderOptions::default()).is_err());
    }

    #[test]
    fn surround_comes_out_in_symphonias_order() {
        // Vorbis order L C R RL RR LFE; symphonia's FL FR FC LFE RL RR.
        let surround = Head::parse(&head(6, 0, 1, &[4, 2, 0, 4, 1, 2, 3, 5])).unwrap();
        let (layout, mapping) = surround.layout();
        assert_eq!(layout.count(), 6);
        assert_eq!(mapping, [0, 1, 4, 5, 2, 3]);
        let stereo = Head::parse(&head(2, 0, 0, &[])).unwrap();
        assert_eq!(stereo.layout().1, [0, 1]);
    }
}
