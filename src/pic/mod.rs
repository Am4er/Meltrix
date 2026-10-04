mod save;
mod wic;

use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};

use crate::Res;

pub const EXTS: [&str; 19] = [
    "jpg", "jpeg", "jfif", "png", "webp", "gif", "bmp", "tif", "tiff", "ico", "tga", "qoi", "ppm", "pgm", "pbm", "pnm",
    "heic", "heif", "avif",
];

#[derive(Clone, Copy, PartialEq)]
pub enum Fmt {
    Webp,
    Png,
    Jpeg,
}

impl Fmt {
    pub const ALL: [Fmt; 3] = [Fmt::Webp, Fmt::Png, Fmt::Jpeg];

    pub fn ext(self) -> &'static str {
        match self {
            Fmt::Webp => "webp",
            Fmt::Png => "png",
            Fmt::Jpeg => "jpg",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Fmt::Webp => "WebP",
            Fmt::Png => "PNG",
            Fmt::Jpeg => "JPG",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Fmt::Webp => "Small files, keeps transparency",
            Fmt::Png => "Lossless, keeps transparency, big files",
            Fmt::Jpeg => "No transparency, uses a white background",
        }
    }
}

pub fn run(src: &Path, out: &Path, fmt: Fmt, q: u8, prog: &AtomicU32) -> Res<()> {
    prog.store(u32::MAX, Ordering::Relaxed);
    let (img, icc) = load(src)?;
    save::write(&img, icc.as_deref(), out, fmt, q)
}

fn load(p: &Path) -> Res<(DynamicImage, Option<Vec<u8>>)> {
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if matches!(ext.as_str(), "heic" | "heif" | "avif") {
        return wic::load(p).map_err(|e| {
            let file = e.downcast_ref::<windows::core::Error>().is_some_and(|w| (w.code().0 as u32) >> 16 == 0x8007);
            if file {
                e
            } else if ext == "avif" {
                "AVIF needs the AV1 Video Extension".into()
            } else {
                "HEIC needs the HEIF and HEVC extensions".into()
            }
        });
    }
    match decode(p) {
        Ok(x) => Ok(x),
        Err(_) => wic::load(p).map_err(|_| "could not read this image".into()),
    }
}

fn decode(p: &Path) -> Res<(DynamicImage, Option<Vec<u8>>)> {
    let mut r = ImageReader::new(Cursor::new(fs::read(p)?));
    if let Ok(f) = ImageFormat::from_path(p) {
        r.set_format(f);
    }
    let mut dec = r.with_guessed_format()?.into_decoder()?;
    let icc = dec.icc_profile().ok().flatten();
    let o = dec.orientation().unwrap_or(Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(dec)?;
    img.apply_orientation(o);
    Ok((img, icc))
}
