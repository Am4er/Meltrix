# Meltrix

A small Windows app that turns videos into MP3 files and converts images to WebP, PNG or JPG. Drop files on the window, pick what you want out and press Convert.

It is a single exe with nothing to install, and everything stays on your PC. Several files are converted at the same time.

## Download

Get `Meltrix.exe` from the [releases page](https://github.com/Am4er/Meltrix/releases) and run it. It needs 64 bit Windows 10 or 11.

## Video to MP3

Reads MP4, M4V, MOV and MKV files with AAC, MP3, FLAC, Vorbis or PCM audio, ALAC in MP4 and M4V, and WebM files with Vorbis audio. You can pick 128, 192, 256 or 320 kbps.

Surround sound in FLAC or PCM is mixed down to stereo. Files with 5.1 AAC, HE-AAC, Opus, AC-3 or DTS audio can't be converted. If a video has more than one audio track, the first one that works is used.

## Images

Reads JPG, PNG, WebP, GIF, BMP, TIFF, ICO, TGA, QOI, PNM, HEIC and AVIF, and saves them as WebP, PNG or JPG with the quality you set.

Photos are rotated the right way using their EXIF orientation and keep their color profile. Transparency stays in WebP and PNG, while JPG gets a white background. For animated GIF and WebP files the first frame is used. Other metadata such as the GPS location is not copied.

HEIC and AVIF are opened through Windows. HEIC needs the HEIF Image Extensions and HEVC Video Extensions from the Microsoft Store, AVIF needs the AV1 Video Extension.

## Where files go

New files are saved next to the originals unless you choose another folder. Nothing gets overwritten: if `clip.mp3` already exists, the new file is saved as `clip (2).mp3`.

## Building

You need Rust and the Visual Studio C++ build tools.

```
cargo build --release
```

The exe ends up in `target/release/meltrix.exe`.

## Built with

[egui](https://github.com/emilk/egui), [Symphonia](https://github.com/pdeljanov/Symphonia), [LAME](https://lame.sourceforge.io/) through [mp3lame-encoder](https://github.com/DoumanAsh/mp3lame-encoder), [image](https://github.com/image-rs/image), [libwebp](https://chromium.googlesource.com/webm/libwebp) through [webp](https://github.com/jaredforth/webp), and [jpeg-encoder](https://github.com/vstroebel/jpeg-encoder).
