#!/bin/sh
# Makes the fixtures of `tests/formats.rs` with ffmpeg: one file per format
# the app offers (see `app/src/lib/media.ts`) and per variant met in practice.
# Every file holds the same signal, so the test knows what it must decode to:
# left a 441 Hz sine at 0.5, right an 882 Hz sine at 0.25 (a swap, a wrong
# gain or a wrong pitch shows). Run from this folder; ffmpeg 6 or later.
set -e

LR='0.5*sin(2*PI*441*t)|0.25*sin(2*PI*882*t)'
# 5.1: front left/right as above, the other channels loud and different.
SURROUND="$LR|0.9*sin(2*PI*3000*t)|0.9*sin(2*PI*50*t)|0.9*sin(2*PI*5000*t)|0.9*sin(2*PI*7000*t)"
MONO='0.5*sin(2*PI*441*t)'

make() {
	src=$1 rate=$2 out=$3
	shift 3
	ffmpeg -v error -y -f lavfi -i "aevalsrc=$src:s=$rate:d=$DUR" -map_metadata -1 -fflags +bitexact "$@" "$out"
}

DUR=0.25
make "$LR" 44100 pcm16.wav -c:a pcm_s16le
make "$LR" 44100 pcm24.wav -c:a pcm_s24le # WAVE_FORMAT_EXTENSIBLE
make "$LR" 48000 float32.wav -c:a pcm_f32le
make "$MONO" 22050 u8-mono.wav -c:a pcm_u8
make "$SURROUND" 44100 surround.wav -c:a pcm_s16le -channel_layout 5.1
make "$LR" 44100 pcm16.aiff -c:a pcm_s16be
make "$LR" 48000 pcm24.aif -c:a pcm_s24be
make "$LR" 48000 pcm24.caf -c:a pcm_s24le
# RF64 (as recorders write it, also when small) and Sony Wave64: read by
# jq-media's `wav64`.
make "$LR" 48000 rf64.wav -c:a pcm_s24le -rf64 always
make "$LR" 44100 float32.w64 -c:a pcm_f32le

DUR=0.5
make "$LR" 44100 flac16.flac -c:a flac -sample_fmt s16
make "$LR" 96000 flac24.flac -c:a flac -sample_fmt s32 -bits_per_raw_sample 24
make "$LR" 44100 alac.m4a -c:a alac
make "$LR" 44100 flac.oga -c:a flac -sample_fmt s16
make "$LR" 44100 aac.m4a -c:a aac -b:a 128k -aac_pns 0
make "$LR" 44100 adts.aac -c:a aac -b:a 128k -aac_pns 0 -f adts
make "$LR" 44100 vorbis.ogg -c:a libvorbis -q:a 4
# Opus in Ogg, offered as .ogg as well: decoded by jq-media's `opus`.
make "$LR" 48000 opus.ogg -c:a libopus -b:a 96k
make "$LR" 48000 opus.opus -c:a libopus -b:a 64k
# 5.1 Opus: libopus reorders the channels into symphonia's order.
make "$SURROUND" 48000 surround.opus -c:a libopus -b:a 256k -channel_layout 5.1 -mapping_family 1
make "$LR" 48000 cbr.mp3 -c:a libmp3lame -b:a 128k
make "$LR" 48000 mp2.mp2 -c:a mp2 -b:a 192k
# MP3 in a WAV file (format tag 0x0055), as broadcast and some DAWs write it.
make "$LR" 44100 mp3-in.wav -c:a libmp3lame -b:a 128k -f wav
# As music libraries tag them: ID3v2 with a title and a cover picture.
ffmpeg -v error -y -f lavfi -i "aevalsrc=$LR:s=44100:d=$DUR" -f lavfi -i "color=c=red:s=32x32:d=0.04" \
	-map 0 -map 1 -fflags +bitexact -c:a libmp3lame -q:a 4 -c:v mjpeg -disposition:v attached_pic \
	-id3v2_version 3 -metadata title="Fixture" -metadata:s:v comment="Cover (front)" tagged.mp3
# Video files: the sound of a clip (a tiny picture keeps them small).
for out in video.mp4 video.mov; do
	ffmpeg -v error -y -f lavfi -i "aevalsrc=$LR:s=48000:d=$DUR" -f lavfi -i "color=c=red:s=32x32:d=$DUR" \
		-map 1 -map 0 -fflags +bitexact -c:v libx264 -c:a aac -b:a 192k -aac_pns 0 "$out"
done
# Matroska is not offered (symphonia gives no length and seeks wrongly);
# this one tells when that changes.
make "$LR" 48000 flac.mka -c:a flac -sample_fmt s16

# Not decoded, but named in the refusal: with the container symphonia reads
# (AC-3 and 5.1 AAC in MP4) and without (WMA, raw AC-3, WavPack).
DUR=0.1
make "$SURROUND" 48000 aac51.m4a -c:a aac -b:a 256k -channel_layout 5.1
make "$LR" 48000 ac3.m4a -c:a ac3
make "$LR" 48000 ac3.ac3 -c:a ac3
make "$LR" 44100 wma.wma -c:a wmav2
make "$LR" 44100 wavpack.wv -c:a wavpack
