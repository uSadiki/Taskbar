//! Decodes the wallpaper with the Windows Imaging Component and paints it
//! the way Windows does on one monitor. WIC reads every format Windows accepts
//! as a wallpaper, so nothing here depends on the file type.
//!
//! Must be called on a thread with COM initialized.

use std::path::Path;

use windows::Win32::Foundation::GENERIC_READ;
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPRGBA, IWICBitmapSource, IWICImagingFactory,
    WICBitmapDitherTypeNone, WICBitmapInterpolationModeFant, WICBitmapPaletteTypeCustom,
    WICDecodeMetadataCacheOnDemand, WICRect,
};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::core::{HSTRING, Interface};

use super::slice::{self, Fit};
use crate::error::{AppError, AppResult};
use crate::geometry::Rect;

/// An opaque RGBA image, 4 bytes per pixel, rows top to bottom.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    /// A single flat colour.
    pub fn solid(width: u32, height: u32, color: [u8; 3]) -> Self {
        let pixel = [color[0], color[1], color[2], 255];
        let rgba = pixel.repeat(width as usize * height as usize);
        Self { width, height, rgba }
    }
}

/// Copy premultiplied RGBA `src` rows onto opaque `dst`, blending any
/// transparency over what is already there (the background colour).
fn composite(dst: &mut Image, src: &[u8], src_w: i32, src_h: i32, dst_x: i32, dst_y: i32) {
    for row in 0..src_h {
        let s = (row * src_w * 4) as usize;
        let d = (((dst_y + row) * dst.width as i32 + dst_x) * 4) as usize;
        let src_row = &src[s..s + (src_w * 4) as usize];
        let dst_row = &mut dst.rgba[d..d + (src_w * 4) as usize];
        for (out, px) in dst_row.chunks_exact_mut(4).zip(src_row.chunks_exact(4)) {
            let alpha = u32::from(px[3]);
            if alpha == 255 {
                out[..3].copy_from_slice(&px[..3]);
            } else if alpha > 0 {
                for c in 0..3 {
                    // Source is premultiplied: out = src + dst * (1 - alpha).
                    let blended = u32::from(px[c]) + u32::from(out[c]) * (255 - alpha) / 255;
                    out[c] = blended.min(255) as u8;
                }
            }
            out[3] = 255;
        }
    }
}

fn copy_region(source: &IWICBitmapSource, region: Rect) -> AppResult<Vec<u8>> {
    let (w, h) = (region.width(), region.height());
    let mut buffer = vec![0u8; (w as usize) * (h as usize) * 4];
    let rect = WICRect { X: region.left, Y: region.top, Width: w, Height: h };
    unsafe { source.CopyPixels(&rect, (w * 4) as u32, &mut buffer)? };
    Ok(buffer)
}

/// Paint the wallpaper at `path` as it appears on `monitor`, on top of
/// `background`. The result is exactly the monitor's size.
pub fn render(
    path: &Path,
    fit: Fit,
    background: [u8; 3],
    monitor: Rect,
    virtual_desktop: Rect,
) -> AppResult<Image> {
    let (mw, mh) = (monitor.width(), monitor.height());
    if mw <= 0 || mh <= 0 {
        return Err(AppError::Other("the monitor has no area".into()));
    }
    let mut out = Image::solid(mw as u32, mh as u32, background);

    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let decoder = factory.CreateDecoderFromFilename(
            &HSTRING::from(path.as_os_str()),
            None,
            GENERIC_READ,
            WICDecodeMetadataCacheOnDemand,
        )?;
        let frame = decoder.GetFrame(0)?;

        let (mut iw, mut ih) = (0u32, 0u32);
        frame.GetSize(&mut iw, &mut ih)?;
        let Some(placement) = slice::place(fit, (iw, ih), monitor, virtual_desktop) else {
            return Ok(out);
        };

        // Premultiplied RGBA, so transparent wallpapers blend over the
        // background colour and the scaler doesn't fringe at alpha edges.
        let converter = factory.CreateFormatConverter()?;
        converter.Initialize(
            &frame,
            &GUID_WICPixelFormat32bppPRGBA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeCustom,
        )?;

        let source: IWICBitmapSource = if (placement.width, placement.height) == (iw, ih) {
            converter.cast()?
        } else {
            let scaler = factory.CreateBitmapScaler()?;
            scaler.Initialize(
                &converter,
                placement.width,
                placement.height,
                WICBitmapInterpolationModeFant,
            )?;
            scaler.cast()?
        };

        let target = Rect::new(0, 0, mw, mh);
        let pieces = slice::blits(&placement, target);
        if placement.tile {
            // Decode the tile once and repeat it from memory.
            let whole = Rect::new(0, 0, placement.width as i32, placement.height as i32);
            let tile = copy_region(&source, whole)?;
            let stride = whole.width() * 4;
            for piece in pieces {
                let (w, h) = (piece.src.width(), piece.src.height());
                let mut part = Vec::with_capacity((w * h * 4) as usize);
                for row in piece.src.top..piece.src.bottom {
                    let start = (row * stride + piece.src.left * 4) as usize;
                    part.extend_from_slice(&tile[start..start + (w * 4) as usize]);
                }
                composite(&mut out, &part, w, h, piece.dst_x, piece.dst_y);
            }
        } else {
            for piece in pieces {
                let pixels = copy_region(&source, piece.src)?;
                composite(
                    &mut out,
                    &pixels,
                    piece.src.width(),
                    piece.src.height(),
                    piece.dst_x,
                    piece.dst_y,
                );
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_image_is_opaque_and_uniform() {
        let image = Image::solid(3, 2, [10, 20, 30]);
        assert_eq!(image.rgba.len(), 24);
        assert!(image.rgba.chunks_exact(4).all(|p| p == [10, 20, 30, 255]));
    }

    #[test]
    fn opaque_pixels_replace_and_transparent_pixels_keep_the_background() {
        let mut dst = Image::solid(3, 1, [100, 100, 100]);
        // Opaque red, fully transparent, half-transparent white (premultiplied).
        let src = [255, 0, 0, 255, 0, 0, 0, 0, 128, 128, 128, 128];
        composite(&mut dst, &src, 3, 1, 0, 0);
        assert_eq!(&dst.rgba[0..4], &[255, 0, 0, 255]);
        assert_eq!(&dst.rgba[4..8], &[100, 100, 100, 255]);
        // 128 + 100 * 127 / 255 = 128 + 49 = 177.
        assert_eq!(&dst.rgba[8..12], &[177, 177, 177, 255]);
    }

    #[test]
    fn composite_respects_the_destination_offset() {
        let mut dst = Image::solid(4, 3, [0, 0, 0]);
        let src = [9, 9, 9, 255, 8, 8, 8, 255];
        composite(&mut dst, &src, 1, 2, 2, 1);
        let at = |x: usize, y: usize| dst.rgba[(y * 4 + x) * 4];
        assert_eq!(at(2, 1), 9);
        assert_eq!(at(2, 2), 8);
        assert_eq!(at(2, 0), 0);
        assert_eq!(at(1, 1), 0);
        assert_eq!(at(3, 2), 0);
    }
}
