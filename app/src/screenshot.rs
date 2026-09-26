//! Development screenshot capture: environment variable `MMP_SCREENSHOT` triggers
//! a single PNG output, then the app closes itself.

use eframe::egui;
use std::path::PathBuf;

/// Captures one screenshot when the `MMP_SCREENSHOT` environment variable is set.
pub struct Screenshotter {
    png: PathBuf,
    frames: u32,
    requested: bool,
    done: bool,
}

impl Screenshotter {
    /// Returns `Some` when `MMP_SCREENSHOT` is set, otherwise `None`.
    pub fn from_env() -> Option<Self> {
        let png = std::env::var_os("MMP_SCREENSHOT")?;
        Some(Self {
            png: PathBuf::from(png),
            frames: 0,
            requested: false,
            done: false,
        })
    }

    /// Call every frame. Waits while `scanning` is true (max 600 frames).
    pub fn tick(&mut self, ctx: &egui::Context, scanning: bool) {
        if self.done {
            return;
        }
        ctx.request_repaint();
        self.frames += 1;

        for ev in ctx.input(|i| i.events.clone()) {
            if let egui::Event::Screenshot { image, .. } = ev {
                if let Err(e) = self.save(&image) {
                    eprintln!("screenshot failed: {e}");
                }
                self.done = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        }

        let ready = (!scanning && self.frames >= 30) || self.frames >= 600;
        if ready && !self.requested {
            self.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                Default::default(),
            ));
        }
    }

    fn save(&self, img: &egui::ColorImage) -> std::io::Result<()> {
        let bmp = self.png.with_extension("bmp");
        std::fs::write(&bmp, encode_bmp(img))?;
        let st = std::process::Command::new("sips")
            .args(["-s", "format", "png"])
            .arg(&bmp)
            .arg("--out")
            .arg(&self.png)
            .output()?;
        let _ = std::fs::remove_file(&bmp);
        if !st.status.success() {
            return Err(std::io::Error::other("sips failed"));
        }
        Ok(())
    }
}

/// Encodes a ColorImage as a 24-bit BMP (bottom-up, 4-byte row padding).
pub fn encode_bmp(img: &egui::ColorImage) -> Vec<u8> {
    let w = img.width();
    let h = img.height();
    let row_size = (w * 3 + 3) & !3; // 4-byte aligned
    let padding = row_size - w * 3;
    let data_size = row_size * h;
    let file_size = 54 + data_size;

    let mut buf = Vec::with_capacity(file_size);

    // BMP file header (14 bytes)
    buf.extend_from_slice(b"BM");
    buf.extend_from_slice(&(file_size as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&54u32.to_le_bytes());

    // BITMAPINFOHEADER (40 bytes)
    buf.extend_from_slice(&40u32.to_le_bytes());
    buf.extend_from_slice(&(w as i32).to_le_bytes());
    buf.extend_from_slice(&(h as i32).to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes());
    buf.extend_from_slice(&24u16.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&(data_size as u32).to_le_bytes());
    buf.extend_from_slice(&2835i32.to_le_bytes());
    buf.extend_from_slice(&2835i32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());

    // Pixel data: bottom-up, BGR, rows padded to 4 bytes
    for y in (0..h).rev() {
        for p in &img.pixels[y * w..(y + 1) * w] {
            buf.extend_from_slice(&[p.b(), p.g(), p.r()]);
        }
        buf.resize(buf.len() + padding, 0);
    }

    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Color32, ColorImage};

    #[test]
    fn encode_bmp_basic() {
        let img = ColorImage::new([2, 1], vec![Color32::RED, Color32::BLUE]);
        let data = encode_bmp(&img);

        // Header: "BM"
        assert_eq!(&data[0..2], b"BM");

        // File length: 54 + (2*3 + 0) = 60
        assert_eq!(data.len(), 54 + 8);

        // First pixel (top-left = RED) → BGR = [0, 0, 255]
        let start = 54usize;
        assert_eq!(data[start], 0); // B
        assert_eq!(data[start + 1], 0); // G
        assert_eq!(data[start + 2], 255); // R
    }
}
