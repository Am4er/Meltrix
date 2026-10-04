use std::path::Path;
use std::ptr;

use image::metadata::Orientation;
use image::{DynamicImage, RgbaImage};
use windows::Win32::Foundation::GENERIC_READ;
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PropVariantClear};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows::Win32::System::Variant::VT_UI2;
use windows::core::{HSTRING, w};

use crate::Res;

pub fn load(p: &Path) -> Res<(DynamicImage, Option<Vec<u8>>)> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let f: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let dec = f.CreateDecoderFromFilename(&HSTRING::from(p), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand)?;
        let frame = dec.GetFrame(0)?;
        let conv = f.CreateFormatConverter()?;
        conv.Initialize(&frame, &GUID_WICPixelFormat32bppRGBA, WICBitmapDitherTypeNone, None::<&IWICPalette>, 0.0, WICBitmapPaletteTypeCustom)?;
        let (mut w, mut h) = (0, 0);
        conv.GetSize(&mut w, &mut h)?;
        let mut buf = vec![0u8; w as usize * h as usize * 4];
        conv.CopyPixels(ptr::null(), w * 4, &mut buf)?;
        let mut img = DynamicImage::ImageRgba8(RgbaImage::from_raw(w, h, buf).ok_or("bad image size")?);
        if let Some(o) = orientation(&frame) {
            img.apply_orientation(o);
        }
        Ok((img, icc(&f, &frame)))
    }
}

fn orientation(frame: &IWICBitmapFrameDecode) -> Option<Orientation> {
    unsafe {
        let q = frame.GetMetadataQueryReader().ok()?;
        let mut v = PROPVARIANT::default();
        q.GetMetadataByName(w!("System.Photo.Orientation"), &mut v).ok()?;
        let a = &v.Anonymous.Anonymous;
        let o = if a.vt == VT_UI2 { Orientation::from_exif(a.Anonymous.uiVal as u8) } else { None };
        let _ = PropVariantClear(&mut v);
        o
    }
}

fn icc(f: &IWICImagingFactory, frame: &IWICBitmapFrameDecode) -> Option<Vec<u8>> {
    unsafe {
        let mut list = [Some(f.CreateColorContext().ok()?)];
        let mut n = 0;
        frame.GetColorContexts(&mut list, &mut n).ok()?;
        let ctx = list[0].as_ref().filter(|_| n > 0)?;
        if ctx.GetType().ok()? != WICColorContextProfile {
            return None;
        }
        let mut size = 0;
        let _ = ctx.GetProfileBytes(&mut [], &mut size);
        if size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        ctx.GetProfileBytes(&mut buf, &mut size).ok()?;
        Some(buf)
    }
}
