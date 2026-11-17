//! App icons as PNG, rendered by the shell at high resolution.
//!
//! Must be called on the shell worker thread.

use std::mem::size_of;

use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, HBITMAP,
    HGDIOBJ, ReleaseDC,
};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromIDList, SHCreateItemFromParsingName,
    SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
};
use windows::core::HSTRING;

use super::pins::ShellRef;
use crate::error::{AppError, AppResult};

/// Icons are requested at this size and scaled down by the dock, so they stay
/// sharp at 150% scaling and when magnified.
pub const ICON_SIZE: i32 = 128;

/// Read a 32-bit bitmap's pixels as straight (not premultiplied) RGBA.
fn bitmap_to_rgba(bitmap: HBITMAP, size: i32) -> AppResult<Vec<u8>> {
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size,
            biHeight: -size, // negative: rows top to bottom
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    let copied = unsafe {
        let dc = GetDC(None);
        let rows = GetDIBits(
            dc,
            bitmap,
            0,
            size as u32,
            Some(pixels.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );
        ReleaseDC(None, dc);
        rows
    };
    if copied == 0 {
        return Err(AppError::Other("could not read the icon bitmap".into()));
    }
    unpremultiply_bgra(&mut pixels);
    Ok(pixels)
}

/// Convert premultiplied BGRA to straight RGBA in place. A bitmap with no
/// alpha channel at all (every alpha byte zero) is treated as opaque.
fn unpremultiply_bgra(pixels: &mut [u8]) {
    let opaque = pixels.as_chunks::<4>().0.iter().all(|px| px[3] == 0);
    for px in pixels.as_chunks_mut::<4>().0 {
        let (b, g, r, a) = (px[0], px[1], px[2], if opaque { 255 } else { px[3] });
        let straight = |c: u8| match a {
            0 => 0,
            255 => c,
            _ => ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8,
        };
        *px = [straight(r), straight(g), straight(b), a];
    }
}

fn encode_png(rgba: &[u8], size: u32) -> AppResult<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, size, size);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| AppError::Other(format!("PNG header: {e}")))?;
    writer
        .write_image_data(rgba)
        .map_err(|e| AppError::Other(format!("PNG data: {e}")))?;
    drop(writer);
    Ok(out)
}

/// Render the icon of a shell item as a PNG.
pub fn render(source: &ShellRef) -> AppResult<Vec<u8>> {
    let factory: IShellItemImageFactory = unsafe {
        match source {
            ShellRef::IdList(id_list) => {
                SHCreateItemFromIDList(id_list.as_ptr() as *const ITEMIDLIST)?
            }
            ShellRef::Path(path) => {
                SHCreateItemFromParsingName(&HSTRING::from(path.as_str()), None)?
            }
        }
    };
    let size = SIZE {
        cx: ICON_SIZE,
        cy: ICON_SIZE,
    };
    let bitmap = unsafe { factory.GetImage(size, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK)? };
    let pixels = bitmap_to_rgba(bitmap, ICON_SIZE);
    unsafe {
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
    }
    encode_png(&pixels?, ICON_SIZE as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swaps_channels_and_undoes_premultiplication() {
        // BGRA: opaque blue, half-transparent white (premultiplied), transparent.
        let mut pixels = vec![255, 0, 0, 255, 128, 128, 128, 128, 0, 0, 0, 0];
        unpremultiply_bgra(&mut pixels);
        assert_eq!(&pixels[0..4], &[0, 0, 255, 255]);
        assert_eq!(&pixels[4..8], &[255, 255, 255, 128]);
        assert_eq!(&pixels[8..12], &[0, 0, 0, 0]);
    }

    #[test]
    fn a_bitmap_without_alpha_is_opaque() {
        // Every alpha byte is zero: an icon drawn without an alpha channel.
        let mut pixels = vec![10, 20, 30, 0, 40, 50, 60, 0];
        unpremultiply_bgra(&mut pixels);
        assert_eq!(pixels, vec![30, 20, 10, 255, 60, 50, 40, 255]);
    }

    #[test]
    fn encodes_a_valid_png() {
        let png = encode_png(&[255u8; 2 * 2 * 4], 2).unwrap();
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    }
}
