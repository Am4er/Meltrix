use std::num::NonZeroU32;

use mp3lame_encoder::{Bitrate, Builder, Encoder, Quality};

use crate::Res;

pub fn make(rate: u32, ch: u8, kbps: u32) -> Res<Encoder> {
    let mut b = Builder::new().ok_or("lame init failed")?;
    b.set_num_channels(ch)?;
    b.set_sample_rate(rate)?;
    if rate > 48_000 && rate.is_multiple_of(44_100) {
        b.set_output_sample_rate(NonZeroU32::new(44_100))?;
    }
    b.set_brate(match kbps {
        128 => Bitrate::Kbps128,
        256 => Bitrate::Kbps256,
        320 => Bitrate::Kbps320,
        _ => Bitrate::Kbps192,
    })?;
    b.set_quality(Quality::VeryNice)?;
    Ok(b.build()?)
}
