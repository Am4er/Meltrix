use std::borrow::Cow;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{DynamicImage, ImageEncoder};
use jpeg_encoder::{ChromaSubsamplingMethod, ColorType, Encoder};

use super::Fmt;
use crate::Res;

pub fn write(img: &DynamicImage, icc: Option<&[u8]>, out: &Path, fmt: Fmt, q: u8) -> Res<()> {
    let mut w = BufWriter::with_capacity(256 * 1024, File::create(out)?);
    match fmt {
        Fmt::Png => png(img, icc, &mut w)?,
        Fmt::Jpeg => jpeg(img, icc, q, &mut w)?,
        Fmt::Webp => webp(img, icc, q, &mut w)?,
    }
    w.flush()?;
    Ok(())
}

fn png(img: &DynamicImage, icc: Option<&[u8]>, w: &mut BufWriter<File>) -> Res<()> {
    let img = match img {
        DynamicImage::ImageRgb32F(_) => Cow::Owned(DynamicImage::ImageRgb16(img.to_rgb16())),
        DynamicImage::ImageRgba32F(_) => Cow::Owned(DynamicImage::ImageRgba16(img.to_rgba16())),
        _ => Cow::Borrowed(img),
    };
    let mut enc = PngEncoder::new_with_quality(w, CompressionType::Fast, FilterType::Adaptive);
    if let Some(icc) = icc_for(icc, !img.color().has_color()) {
        let _ = enc.set_icc_profile(icc.to_vec());
    }
    img.write_with_encoder(enc)?;
    Ok(())
}

fn jpeg(img: &DynamicImage, icc: Option<&[u8]>, q: u8, w: &mut BufWriter<File>) -> Res<()> {
    let (Ok(width), Ok(height)) = (u16::try_from(img.width()), u16::try_from(img.height())) else {
        return Err("too big for JPEG".into());
    };
    let gray = !img.color().has_color() && !img.color().has_alpha();
    let mut enc = Encoder::new(w, q);
    enc.set_optimized_huffman_tables(true);
    enc.set_chroma_subsampling_method(ChromaSubsamplingMethod::Average);
    if let Some(icc) = icc_for(icc, gray) {
        enc.add_icc_profile(icc)?;
    }
    if gray {
        let px = match img {
            DynamicImage::ImageLuma8(b) => Cow::Borrowed(b.as_raw()),
            _ => Cow::Owned(img.to_luma8().into_raw()),
        };
        enc.encode(&px, width, height, ColorType::Luma)?;
    } else {
        enc.encode(&rgb(img), width, height, ColorType::Rgb)?;
    }
    Ok(())
}

fn webp(img: &DynamicImage, icc: Option<&[u8]>, q: u8, w: &mut BufWriter<File>) -> Res<()> {
    let (width, height) = (img.width(), img.height());
    if width > 16383 || height > 16383 {
        return Err("too big for WebP".into());
    }
    let mut cfg = webp::WebPConfig::new().map_err(|_| "webp init failed")?;
    cfg.quality = q as f32;
    cfg.method = 2;
    let mem = if img.color().has_alpha() {
        let px = match img {
            DynamicImage::ImageRgba8(b) => Cow::Borrowed(b.as_raw()),
            _ => Cow::Owned(img.to_rgba8().into_raw()),
        };
        webp::Encoder::from_rgba(&px, width, height).encode_advanced(&cfg)
    } else {
        webp::Encoder::from_rgb(&rgb(img), width, height).encode_advanced(&cfg)
    }
    .map_err(|e| format!("webp encode failed: {e:?}"))?;

    match icc_for(icc, false) {
        Some(icc) => w.write_all(&with_icc(&mem, icc, width, height))?,
        None => w.write_all(&mem)?,
    }
    Ok(())
}

fn with_icc(webp: &[u8], icc: &[u8], w: u32, h: u32) -> Vec<u8> {
    let body = &webp[12..];
    let mut out = Vec::with_capacity(webp.len() + icc.len() + 32);
    out.extend_from_slice(b"RIFF\0\0\0\0WEBP");
    let rest = if body.starts_with(b"VP8X") {
        out.extend_from_slice(&body[..18]);
        out[20] |= 0x20;
        &body[18..]
    } else {
        out.extend_from_slice(b"VP8X");
        out.extend_from_slice(&10u32.to_le_bytes());
        out.extend_from_slice(&[0x20, 0, 0, 0]);
        out.extend_from_slice(&(w - 1).to_le_bytes()[..3]);
        out.extend_from_slice(&(h - 1).to_le_bytes()[..3]);
        body
    };
    out.extend_from_slice(b"ICCP");
    out.extend_from_slice(&(icc.len() as u32).to_le_bytes());
    out.extend_from_slice(icc);
    if icc.len() % 2 == 1 {
        out.push(0);
    }
    out.extend_from_slice(rest);
    let size = (out.len() - 8) as u32;
    out[4..8].copy_from_slice(&size.to_le_bytes());
    out
}

fn icc_for(icc: Option<&[u8]>, gray: bool) -> Option<&[u8]> {
    let want: &[u8] = if gray { b"GRAY" } else { b"RGB " };
    icc.filter(|p| p.get(16..20) == Some(want))
}

fn rgb(img: &DynamicImage) -> Cow<'_, [u8]> {
    if let DynamicImage::ImageRgb8(b) = img {
        return Cow::Borrowed(b.as_raw());
    }
    if !img.color().has_alpha() {
        return Cow::Owned(img.to_rgb8().into_raw());
    }
    let src = match img {
        DynamicImage::ImageRgba8(b) => Cow::Borrowed(b.as_raw()),
        _ => Cow::Owned(img.to_rgba8().into_raw()),
    };
    let mut out = Vec::with_capacity(src.len() / 4 * 3);
    for p in src.as_chunks::<4>().0 {
        let a = p[3] as u32;
        for &c in &p[..3] {
            out.push(((c as u32 * a + 255 * (255 - a) + 127) / 255) as u8);
        }
    }
    Cow::Owned(out)
}
