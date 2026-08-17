use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::RenderImage;

pub const PROBE_BATCH: usize = 4;

#[derive(Clone)]
pub struct Probed {
    pub path: PathBuf,

    pub duration_seconds: Option<f64>,

    pub frame_rate: Option<f32>,
    pub image: Option<Arc<RenderImage>>,
}

pub fn to_bgra(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let expected = (width as usize) * (height as usize) * 4;
    if width == 0 || height == 0 || rgba.len() < expected {
        return None;
    }
    let mut bgra = rgba[..expected].to_vec();
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Some(bgra)
}

pub fn to_render_image(width: u32, height: u32, rgba: &[u8]) -> Option<Arc<RenderImage>> {
    let bgra = to_bgra(width, height, rgba)?;
    let buffer = image::ImageBuffer::from_raw(width, height, bgra)?;
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
}

pub fn poster_at(duration_seconds: f64) -> f64 {
    if !duration_seconds.is_finite() || duration_seconds <= 0.0 {
        return 0.0;
    }
    (duration_seconds * 0.1).min(3.0)
}

pub fn probe(path: &Path) -> Probed {
    let info = video::decode::probe(path).ok();
    let duration = info.as_ref().map(|info| info.duration_seconds);
    let frame_rate = info.as_ref().and_then(|info| {
        if info.duration_seconds > 0.0 && info.frame_count > 0 {
            Some(info.frame_count as f32 / info.duration_seconds as f32)
        } else {
            None
        }
    });
    let image = duration
        .map(poster_at)
        .and_then(|at| frame_at(path, at))
        .or_else(|| first_frame(path));

    Probed {
        path: path.to_path_buf(),
        duration_seconds: duration.filter(|seconds| *seconds > 0.0),
        frame_rate: frame_rate.filter(|rate| rate.is_finite() && *rate > 0.0),
        image,
    }
}

pub fn first_frame(path: &Path) -> Option<Arc<RenderImage>> {
    let frame = video::decode::first_frame(path).ok()?;
    to_render_image(frame.width as u32, frame.height as u32, &frame.rgba)
}

pub fn frame_at(path: &Path, seconds: f64) -> Option<Arc<RenderImage>> {
    let frame = video::decode::frame_at(path, seconds.max(0.0)).ok()?;
    to_render_image(frame.width as u32, frame.height as u32, &frame.rgba)
}

pub fn fit_within(frame: cutix_playback::SourceFrame, limit: Option<u32>) -> (u32, u32, Vec<u8>) {
    let Some(limit) = limit.filter(|limit| *limit > 0) else {
        return (frame.width, frame.height, frame.rgba);
    };
    if frame.width <= limit || frame.width == 0 || frame.height == 0 {
        return (frame.width, frame.height, frame.rgba);
    }

    let width = limit;
    let height = ((frame.height as u64 * width as u64) / frame.width as u64).max(1) as u32;
    let mut pixels = Vec::with_capacity((width as usize) * (height as usize) * 4);
    for row in 0..height {
        let source_row = (row as u64 * frame.height as u64 / height as u64) as u32;
        for column in 0..width {
            let source_column = (column as u64 * frame.width as u64 / width as u64) as u32;
            let offset =
                ((source_row as usize) * (frame.width as usize) + source_column as usize) * 4;
            pixels.extend_from_slice(&frame.rgba[offset..offset + 4]);
        }
    }
    (width, height, pixels)
}

pub struct FrameOut {
    pub generation: u64,

    pub struggling: bool,

    pub timestamp: f64,
    pub image: std::sync::Arc<gpui::RenderImage>,
}

struct FrameRequest {
    path: PathBuf,
    at: f64,
    pub generation: u64,

    width: Option<u32>,
}

pub struct FrameWorker {
    requests: std::sync::mpsc::Sender<FrameRequest>,
    latest: std::sync::Arc<std::sync::Mutex<Option<FrameOut>>>,

    failed: std::sync::Arc<std::sync::Mutex<Vec<PathBuf>>>,
}

impl FrameWorker {
    pub fn spawn() -> Option<Self> {
        let (requests, incoming) = std::sync::mpsc::channel::<FrameRequest>();
        let latest: std::sync::Arc<std::sync::Mutex<Option<FrameOut>>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));
        let worker = std::sync::Arc::clone(&latest);
        let failed: std::sync::Arc<std::sync::Mutex<Vec<PathBuf>>> =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let worker_failed = std::sync::Arc::clone(&failed);

        std::thread::Builder::new()
            .name("cutix-hover-preview".into())
            .spawn(move || {
                let mut cache = cutix_playback::DecodeCache::new();
                while let Ok(mut request) = incoming.recv() {
                    while let Ok(newer) = incoming.try_recv() {
                        request = newer;
                    }
                    cache.begin_frame();
                    let key = request.path.to_string_lossy().to_string();
                    let Ok(frame) = cache.video_frame(&key, &request.path, request.at.max(0.0))
                    else {
                        let mut failed = worker_failed
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        failed.push(request.path.clone());
                        continue;
                    };
                    let timestamp = frame.timestamp;
                    let (width, height, rgba) = fit_within(frame, request.width);
                    let Some(image) = to_render_image(width, height, &rgba) else {
                        continue;
                    };
                    let mut slot = worker
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    *slot = Some(FrameOut {
                        generation: request.generation,
                        struggling: cache.struggling(&key),
                        timestamp,
                        image,
                    });
                }
            })
            .ok()?;

        Some(Self {
            requests,
            latest,
            failed,
        })
    }

    pub fn request(&self, path: PathBuf, at: f64, generation: u64, width: Option<u32>) {
        let _ = self.requests.send(FrameRequest {
            path,
            at,
            generation,
            width,
        });
    }

    pub fn take_failures(&self) -> Vec<PathBuf> {
        std::mem::take(
            &mut *self
                .failed
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        )
    }

    pub fn take(&self) -> Option<FrameOut> {
        self.latest
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    }
}

#[derive(Default)]
pub struct Thumbnails {
    images: HashMap<PathBuf, Arc<RenderImage>>,
    durations: HashMap<PathBuf, f64>,
    rates: HashMap<PathBuf, f32>,

    attempted: HashSet<PathBuf>,
    in_flight: usize,
}

impl Thumbnails {
    pub fn image(&self, path: &Path) -> Option<Arc<RenderImage>> {
        self.images.get(path).cloned()
    }

    pub fn duration(&self, path: &Path) -> Option<f64> {
        self.durations.get(path).copied()
    }

    pub fn frame_rate(&self, path: &Path) -> Option<f32> {
        self.rates.get(path).copied()
    }

    pub fn claim<'a>(&mut self, paths: impl Iterator<Item = &'a Path>) -> Vec<PathBuf> {
        if self.in_flight > 0 {
            return Vec::new();
        }
        let batch: Vec<PathBuf> = paths
            .filter(|path| !self.attempted.contains(*path))
            .take(PROBE_BATCH)
            .map(Path::to_path_buf)
            .collect();
        for path in &batch {
            self.attempted.insert(path.clone());
        }
        self.in_flight = batch.len();
        batch
    }

    pub fn store(&mut self, probed: Probed) {
        self.in_flight = self.in_flight.saturating_sub(1);
        if let Some(duration) = probed.duration_seconds {
            self.durations.insert(probed.path.clone(), duration);
        }
        if let Some(rate) = probed.frame_rate {
            self.rates.insert(probed.path.clone(), rate);
        }
        if let Some(image) = probed.image {
            self.images.insert(probed.path, image);
        }
    }

    pub fn clear(&mut self) {
        self.images.clear();
        self.durations.clear();
        self.rates.clear();
        self.attempted.clear();
        self.in_flight = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probed(name: &str, duration: Option<f64>) -> Probed {
        Probed {
            path: PathBuf::from(name),
            duration_seconds: duration,
            frame_rate: None,
            image: None,
        }
    }

    #[test]
    fn the_poster_is_taken_a_little_way_in_rather_than_from_the_black_first_frame() {
        assert_eq!(
            poster_at(60.0),
            3.0,
            "capped, so a long film is not seeked far into"
        );
        assert_eq!(poster_at(10.0), 1.0);
        assert!((poster_at(2.0) - 0.2).abs() < 1e-9);
    }

    #[test]
    fn a_file_with_no_readable_duration_is_posted_from_its_very_start() {
        assert_eq!(poster_at(0.0), 0.0);
        assert_eq!(poster_at(-5.0), 0.0);
        assert_eq!(poster_at(f64::NAN), 0.0);
    }

    #[test]
    fn a_file_is_only_ever_probed_once_even_when_it_cannot_be_read() {
        let mut cache = Thumbnails::default();
        let paths = [PathBuf::from("a.mp4"), PathBuf::from("b.mp4")];

        let batch = cache.claim(paths.iter().map(PathBuf::as_path));
        assert_eq!(batch.len(), 2);

        assert!(cache.claim(paths.iter().map(PathBuf::as_path)).is_empty());

        cache.store(probed("a.mp4", None));
        cache.store(probed("b.mp4", None));
        assert!(cache.claim(paths.iter().map(PathBuf::as_path)).is_empty());
        assert!(cache.duration(Path::new("a.mp4")).is_none());
    }

    #[test]
    fn a_probed_duration_is_remembered_for_the_card_to_show() {
        let mut cache = Thumbnails::default();
        cache.claim(std::iter::once(Path::new("clip.mp4")));
        cache.store(probed("clip.mp4", Some(12.5)));
        assert_eq!(cache.duration(Path::new("clip.mp4")), Some(12.5));
        assert!(cache.image(Path::new("clip.mp4")).is_none());
    }

    #[test]
    fn a_batch_is_bounded_so_a_big_folder_does_not_open_every_file_at_once() {
        let paths: Vec<PathBuf> = (0..50)
            .map(|index| PathBuf::from(format!("clip{index}.mp4")))
            .collect();
        let mut cache = Thumbnails::default();
        let batch = cache.claim(paths.iter().map(PathBuf::as_path));
        assert_eq!(batch.len(), PROBE_BATCH);
    }

    #[test]
    fn changing_folder_forgets_what_the_old_one_looked_like() {
        let mut cache = Thumbnails::default();
        cache.claim(std::iter::once(Path::new("clip.mp4")));
        cache.store(probed("clip.mp4", Some(3.0)));

        cache.clear();
        assert!(cache.duration(Path::new("clip.mp4")).is_none());

        assert_eq!(cache.claim(std::iter::once(Path::new("clip.mp4"))).len(), 1);
    }

    #[test]
    fn a_frame_whose_pixels_are_short_or_empty_yields_no_image_rather_than_a_panic() {
        assert!(to_render_image(2, 2, &[0; 8]).is_none(), "half the pixels");
        assert!(to_render_image(0, 0, &[]).is_none());
        assert!(to_render_image(1, 1, &[1, 2, 3, 4]).is_some());
    }

    #[test]
    fn the_channel_order_is_swapped_for_the_compositor() {
        let pixel = to_bgra(1, 1, &[255, 0, 0, 255]).expect("pixels");
        assert_eq!(pixel, vec![0, 0, 255, 255]);

        let two = to_bgra(2, 1, &[1, 2, 3, 4, 5, 6, 7, 8]).expect("pixels");
        assert_eq!(two, vec![3, 2, 1, 4, 7, 6, 5, 8]);
    }
}
