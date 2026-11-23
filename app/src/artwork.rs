//! Album artwork textures: background loading + in-memory LRU.

use crate::style::subtle_fill;
use eframe::egui;
use mmp_core::artwork::cache::ArtworkCache;
use mmp_core::artwork::loader::ArtworkLoader;
use std::collections::HashMap;
use std::path::Path;

/// Maximum number of artwork textures kept in memory.
pub const MAX_TEXTURES: usize = 200;

struct Entry {
    tex: Option<egui::TextureHandle>,
    pending: bool,
    last_used: u64,
}

/// album_key -> texture, loaded in the background and evicted least-recently-used.
pub struct ArtTextures {
    loader: Option<ArtworkLoader>,
    entries: HashMap<String, Entry>,
    tick: u64,
    cap: usize,
}

impl ArtTextures {
    pub fn new(ctx: &egui::Context) -> Self {
        let loader = mmp_core::paths::artwork_dir()
            .ok()
            .and_then(|d| ArtworkCache::new(d).ok())
            .map(|cache| {
                let ctx = ctx.clone();
                ArtworkLoader::new(cache, Box::new(move || ctx.request_repaint()))
            });
        Self {
            loader,
            entries: HashMap::new(),
            tick: 0,
            cap: MAX_TEXTURES,
        }
    }

    /// Texture for the album if loaded; otherwise requests it once and returns None.
    pub fn get(&mut self, album_key: &str, track: &Path) -> Option<egui::TextureHandle> {
        self.tick += 1;
        if let Some(e) = self.entries.get_mut(album_key) {
            e.last_used = self.tick;
            return e.tex.clone();
        }
        if let Some(l) = &self.loader {
            l.request(album_key, track);
        }
        self.entries.insert(
            album_key.to_string(),
            Entry {
                tex: None,
                pending: true,
                last_used: self.tick,
            },
        );
        None
    }

    /// Uploads finished thumbnails and evicts old ones. Call every frame.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let Some(l) = &self.loader else { return };
        for (key, thumb) in l.try_results() {
            let tex = thumb.map(|t| {
                let img = egui::ColorImage::from_rgba_unmultiplied(
                    [t.width as usize, t.height as usize],
                    &t.rgba,
                );
                ctx.load_texture(format!("art:{key}"), img, egui::TextureOptions::LINEAR)
            });
            if let Some(e) = self.entries.get_mut(&key) {
                e.tex = tex;
                e.pending = false;
            }
        }
        evict(&mut self.entries, self.cap);
    }
}

/// Removes least-recently-used finished entries until at most cap remain (pending ones are kept).
fn evict(entries: &mut HashMap<String, Entry>, cap: usize) {
    while entries.len() > cap {
        let oldest = entries
            .iter()
            .filter(|(_, e)| !e.pending)
            .min_by_key(|(_, e)| e.last_used)
            .map(|(k, _)| k.clone());
        match oldest {
            Some(k) => {
                entries.remove(&k);
            }
            None => break,
        }
    }
}

/// Draws album artwork in a size x size square (fitted, rounded), or a placeholder.
pub fn art_image(ui: &mut egui::Ui, tex: Option<&egui::TextureHandle>, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let radius = (size * 0.06).max(4.0);
    match tex {
        Some(t) => {
            let s = t.size_vec2();
            let scale = size / s.x.max(s.y).max(1.0);
            let fitted = egui::Rect::from_center_size(rect.center(), s * scale);
            egui::Image::new(t)
                .corner_radius(radius)
                .paint_at(ui, fitted);
        }
        None => {
            ui.painter().rect_filled(rect, radius, subtle_fill(ui, 20));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(last: u64, pending: bool) -> Entry {
        Entry {
            tex: None,
            pending,
            last_used: last,
        }
    }

    #[test]
    fn evict_removes_oldest_finished() {
        let mut m = HashMap::new();
        m.insert("a".to_string(), entry(1, false));
        m.insert("b".to_string(), entry(2, true));
        m.insert("c".to_string(), entry(3, false));
        m.insert("d".to_string(), entry(4, false));
        evict(&mut m, 2);
        let mut keys: Vec<_> = m.keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, vec!["b".to_string(), "d".to_string()]);
    }

    #[test]
    fn evict_keeps_pending() {
        let mut m = HashMap::new();
        m.insert("a".to_string(), entry(1, true));
        m.insert("b".to_string(), entry(2, true));
        m.insert("c".to_string(), entry(3, true));
        evict(&mut m, 1);
        assert_eq!(m.len(), 3);
    }
}
