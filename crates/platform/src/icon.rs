//! The Snakecharmer app icon, drawn at runtime with GDI+ (no asset files, no
//! new dependencies): a light S-curve "snake" mark inside a dark teal-green
//! disc. Used for the tray notification icon and the settings window's
//! titlebar/class icon.
//!
//! The mark is drawn once into a 32×32 premultiplied-ARGB GDI+ bitmap
//! (anti-aliased, with a real alpha channel so the disc is round rather than
//! boxed) and converted to a GDI `HICON` via `GdipCreateHICONFromBitmap`. The
//! HICON copies its pixels out of GDI+, so it outlives GDI+ shutdown; the
//! caller owns it and frees it with `DestroyIcon`.

use windows_sys::Win32::Graphics::GdiPlus::{
    GdipAddPathBezier, GdipCreateBitmapFromScan0, GdipCreateHICONFromBitmap, GdipCreatePath,
    GdipCreatePen1, GdipCreateSolidFill, GdipDeleteBrush, GdipDeleteGraphics, GdipDeletePath,
    GdipDeletePen, GdipDisposeImage, GdipDrawPath, GdipFillEllipseI, GdipGetImageGraphicsContext,
    GdipSetPenEndCap, GdipSetPenStartCap, GdipSetSmoothingMode, FillModeAlternate, GpBitmap,
    GpBrush, GpGraphics, GpImage, GpPath, GpPen, LineCapRound, SmoothingModeAntiAlias, UnitPixel,
};
use windows_sys::Win32::UI::WindowsAndMessaging::HICON;

/// `PixelFormat32bppARGB` (windows-sys doesn't surface the named constant):
/// 32 bits per pixel, straight (non-premultiplied) alpha.
const PIXEL_FORMAT_32BPP_ARGB: i32 = 0x0026_200A;

/// Icon canvas edge in pixels. Windows scales this down to 16×16 for the tray
/// and small titlebar slots; the bold S and generous disc keep it legible.
const SIZE: i32 = 32;

/// Dark teal-green disc (0xAARRGGBB).
const DISC: u32 = 0xFF15_7A5B;
/// Light mint S-curve.
const CURVE: u32 = 0xFFF2_FBF6;

/// Create the app `HICON`. Requires GDI+ to be initialized by the caller
/// (see [`crate::diagram::startup`]); returns null on any failure. The caller
/// owns the returned icon and must free it with `DestroyIcon`.
///
/// # Safety
/// GDI+ must be active for the duration of the call.
pub unsafe fn create_app_icon() -> HICON {
    let mut bitmap: *mut GpBitmap = std::ptr::null_mut();
    if GdipCreateBitmapFromScan0(SIZE, SIZE, 0, PIXEL_FORMAT_32BPP_ARGB, std::ptr::null(), &mut bitmap)
        != 0
        || bitmap.is_null()
    {
        return std::ptr::null_mut();
    }

    let mut graphics: *mut GpGraphics = std::ptr::null_mut();
    if GdipGetImageGraphicsContext(bitmap as *mut GpImage, &mut graphics) == 0 && !graphics.is_null()
    {
        GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias);

        // Filled disc, inset one pixel so the anti-aliased edge isn't clipped.
        let mut disc_brush: *mut GpBrush = std::ptr::null_mut();
        if GdipCreateSolidFill(DISC, &mut disc_brush as *mut _ as *mut _) == 0 {
            GdipFillEllipseI(graphics, disc_brush, 1, 1, SIZE - 2, SIZE - 2);
            GdipDeleteBrush(disc_brush);
        }

        // S-curve: two cubic segments through the disc, top-right -> bottom-left,
        // with round caps so the stroke reads as a snake at 16 px.
        let mut path: *mut GpPath = std::ptr::null_mut();
        if GdipCreatePath(FillModeAlternate, &mut path) == 0 {
            // Segment 1: upper bowl.
            GdipAddPathBezier(path, 22.0, 8.0, 9.0, 8.0, 9.0, 16.0, 16.0, 16.0);
            // Segment 2: lower bowl (mirrored).
            GdipAddPathBezier(path, 23.0, 16.0, 23.0, 24.0, 10.0, 24.0, 10.0, 24.0);

            let mut pen: *mut GpPen = std::ptr::null_mut();
            if GdipCreatePen1(CURVE, 3.6, UnitPixel, &mut pen) == 0 {
                GdipSetPenStartCap(pen, LineCapRound);
                GdipSetPenEndCap(pen, LineCapRound);
                GdipDrawPath(graphics, pen, path);
                GdipDeletePen(pen);
            }
            GdipDeletePath(path);
        }

        GdipDeleteGraphics(graphics);
    }

    let mut hicon: HICON = std::ptr::null_mut();
    GdipCreateHICONFromBitmap(bitmap, &mut hicon);
    GdipDisposeImage(bitmap as *mut GpImage);
    hicon
}
