mod lame;
mod mix;

use std::error::Error as StdError;
use std::fs::File;
use std::io::{BufWriter, ErrorKind, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use mp3lame_encoder::{max_required_buffer_size, DualPcm, Encoder, FlushGap, MonoPcm};
use symphonia::core::audio::{Audio, GenericAudioBufferRef};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::codecs::audio::well_known::{CODEC_ID_AAC, CODEC_ID_AC3, CODEC_ID_DCA, CODEC_ID_EAC3, CODEC_ID_OPUS};
use symphonia::core::errors::Error;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::Res;

pub const EXTS: [&str; 5] = ["mp4", "m4v", "mov", "mkv", "webm"];

pub fn run(src: &Path, out: &Path, kbps: u32, stop: &AtomicBool, prog: &AtomicU32) -> Res<()> {
    let mss = MediaSourceStream::new(Box::new(File::open(src)?), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = src.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut fmt = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|_| "could not read this file")?;
    let codecs = symphonia::default::get_codecs();

    let mut pick = None;
    let mut fail = None;
    for t in fmt.tracks() {
        let Some(a) = t.codec_params.as_ref().and_then(|c| c.audio()) else {
            continue;
        };
        if codecs.get_audio_decoder(a.codec).is_none() {
            continue;
        }
        match codecs.make_audio_decoder(a, &AudioDecoderOptions::default()) {
            Ok(d) => {
                pick = Some((t.id, t.time_base, t.duration, d));
                break;
            }
            Err(e) => {
                if fail.is_none() {
                    fail = Some((a.codec, e));
                }
            }
        }
    }
    let Some((id, tb, dur, mut dec)) = pick else {
        return Err(match fail {
            Some((CODEC_ID_AAC, Error::Unsupported(_))) => "5.1 and HE-AAC audio isn't supported".into(),
            Some((_, e)) => e.into(),
            None => no_audio(&*fmt),
        });
    };
    let info = fmt.media_info();
    let total = tb
        .zip(dur)
        .and_then(|(b, d)| b.calc_duration(d))
        .or_else(|| info.time_base.zip(info.duration).and_then(|(b, d)| b.calc_duration(d)))
        .map(|t| t.as_secs_f64())
        .filter(|&t| t > 0.0);

    prog.store(if total.is_some() { 0 } else { u32::MAX }, Ordering::Relaxed);

    let mut w = BufWriter::with_capacity(256 * 1024, File::create(out)?);
    let mut enc: Option<Encoder> = None;
    let mut spec = (0, 0);
    let mut gains = Vec::new();
    let mut planes: Vec<Vec<f32>> = Vec::new();
    let (mut l, mut r) = (Vec::new(), Vec::new());
    let mut mp3 = Vec::new();

    loop {
        let pkt = match fmt.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) | Err(Error::ResetRequired) => break,
            Err(Error::IoError(e)) if e.kind() == ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        };
        if stop.load(Ordering::Relaxed) {
            return Err("stopped".into());
        }
        if pkt.track_id != id {
            continue;
        }
        if let Some(tb) = tb
            && let Some(total) = total
            && let Some(t) = tb.calc_time(pkt.pts)
        {
            prog.store((t.as_secs_f64() / total * 1000.0).clamp(0.0, 1000.0) as u32, Ordering::Relaxed);
        }

        let buf = match dec.decode(&pkt) {
            Ok(b) => b,
            Err(Error::DecodeError(_) | Error::IoError(_)) => continue,
            Err(e) => return Err(e.into()),
        };
        let n = buf.frames();
        if n == 0 {
            continue;
        }

        let ch = buf.spec().channels().count();
        let rate = buf.spec().rate();
        if enc.is_none() {
            enc = Some(lame::make(rate, ch.min(2) as u8, kbps)?);
            spec = (rate, ch);
            gains = mix::gains(buf.spec().channels());
        } else if spec != (rate, ch) {
            return Err("audio format changes in the middle of the file".into());
        }
        let e = enc.as_mut().unwrap();

        let (a, b): (&[f32], Option<&[f32]>) = match &buf {
            GenericAudioBufferRef::F32(x) if ch <= 2 => (x.plane(0).unwrap(), x.plane(1)),
            _ => {
                buf.copy_to_vecs_planar(&mut planes);
                if ch > 2 {
                    mix::downmix(&planes, &gains, &mut l, &mut r);
                    (&l[..], Some(&r[..]))
                } else {
                    (&planes[0][..], planes.get(1).map(|p| &p[..]))
                }
            }
        };

        mp3.clear();
        mp3.reserve(max_required_buffer_size(n));
        match b {
            Some(right) => e.encode_to_vec(DualPcm { left: a, right }, &mut mp3)?,
            None => e.encode_to_vec(MonoPcm(a), &mut mp3)?,
        };
        w.write_all(&mp3)?;
    }

    let Some(mut e) = enc else {
        return Err("no audio could be decoded".into());
    };
    mp3.clear();
    mp3.reserve(16 * 1024);
    e.flush_to_vec::<FlushGap>(&mut mp3)?;
    w.write_all(&mp3)?;

    let mut f = w.into_inner().map_err(|e| e.into_error())?;
    mp3.clear();
    mp3.reserve(e.lame_tag_size());
    if e.lame_tag_encode_to_vec(&mut mp3).is_some() {
        f.seek(SeekFrom::Start(0))?;
        f.write_all(&mp3)?;
    }
    prog.store(1000, Ordering::Relaxed);
    Ok(())
}

fn no_audio(fmt: &dyn FormatReader) -> Box<dyn StdError + Send + Sync> {
    let codec = fmt.tracks().iter().find_map(|t| t.codec_params.as_ref()?.audio()).map(|a| a.codec);
    match codec {
        None => "no audio track in this file".into(),
        Some(CODEC_ID_OPUS) => "Opus audio isn't supported".into(),
        Some(CODEC_ID_AC3 | CODEC_ID_EAC3) => "Dolby AC-3 audio isn't supported".into(),
        Some(CODEC_ID_DCA) => "DTS audio isn't supported".into(),
        Some(_) => "this audio format isn't supported".into(),
    }
}
