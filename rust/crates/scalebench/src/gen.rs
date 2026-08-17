use cutix_project::model::{TimelineElement, Track};
use cutix_project::Project;
use serde_json::{json, Value};
use time::MediaTime;

pub struct Shape {
    pub elements: usize,
    pub media_files: usize,
    pub minutes: f64,
    pub overlay_tracks: usize,
    pub audio_tracks: usize,
    pub keyframes_per_channel: usize,
    pub effects_per_element: usize,
}

impl Default for Shape {
    fn default() -> Self {
        Self {
            elements: 500,
            media_files: 100,
            minutes: 30.0,
            overlay_tracks: 12,
            audio_tracks: 4,
            keyframes_per_channel: 8,
            effects_per_element: 3,
        }
    }
}

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, limit: usize) -> usize {
        if limit == 0 {
            0
        } else {
            (self.next() % limit as u64) as usize
        }
    }

    fn unit(&mut self) -> f64 {
        (self.next() % 1_000_000) as f64 / 1_000_000.0
    }
}

fn seconds(value: f64) -> i64 {
    MediaTime::from_seconds_f64(value)
        .unwrap_or(MediaTime::ZERO)
        .as_ticks()
}

fn animations(rng: &mut Rng, start: f64, duration: f64, keys: usize) -> Value {
    let mut channels = serde_json::Map::new();
    for property in ["opacity", "transform.scaleX", "transform.position.x"] {
        let mut list = Vec::new();
        for index in 0..keys {
            let fraction = index as f64 / keys.max(1) as f64;
            list.push(json!({
                "id": format!("k{index}-{property}"),
                "time": seconds(start + duration * fraction),
                "value": rng.unit(),
                "segmentToNext": "bezier",
                "tangentMode": "auto",
                "leftHandle": { "dt": seconds(0.1), "dv": 0.1 },
                "rightHandle": { "dt": seconds(0.1), "dv": -0.1 }
            }));
        }
        channels.insert(
            property.to_string(),
            json!({ "kind": "scalar", "keys": list }),
        );
    }
    json!({ "bindings": {}, "channels": channels })
}

fn effects(rng: &mut Rng, count: usize) -> Value {
    let kinds = [
        "brightness",
        "contrast",
        "saturation",
        "blur",
        "sharpen",
        "vignette",
    ];
    let mut list = Vec::new();
    for index in 0..count {
        let kind = kinds[rng.below(kinds.len())];
        list.push(json!({
            "id": format!("fx{index}-{kind}"),
            "type": kind,
            "params": { "amount": rng.unit(), "radius": rng.unit() * 20.0 },
            "enabled": true
        }));
    }
    json!(list)
}

fn masks(rng: &mut Rng) -> Value {
    json!([{
        "id": "mask0",
        "type": if rng.below(2) == 0 { "ellipse" } else { "rectangle" },
        "params": {
            "x": rng.unit(), "y": rng.unit(),
            "width": rng.unit(), "height": rng.unit(),
            "feather": rng.unit() * 30.0, "invert": false
        }
    }])
}

fn transform() -> Value {
    json!({ "scaleX": 1.0, "scaleY": 1.0, "position": { "x": 0.0, "y": 0.0 }, "rotate": 0.0 })
}

fn transition(rng: &mut Rng) -> Value {
    let kinds = ["fade", "wipe", "slide", "dissolve"];
    json!({
        "type": kinds[rng.below(kinds.len())],
        "duration": seconds(0.5),
        "easing": "easeInOut"
    })
}

pub fn build(shape: &Shape, seed: u64) -> Project {
    let mut rng = Rng::new(seed);
    let mut project = Project::new("scalebench", "1970-01-01T00:00:00.000Z".into());
    let span = shape.minutes * 60.0;

    let scene = project.scenes.first_mut().expect("scene");
    for index in 0..shape.overlay_tracks {
        scene.tracks.overlay.push(Track::Video {
            id: format!("overlay-{index}"),
            name: format!("Overlay {index}"),
            elements: Vec::new(),
            muted: false,
            hidden: false,
        });
    }
    for index in 0..shape.audio_tracks {
        scene.tracks.audio.push(Track::Audio {
            id: format!("audio-{index}"),
            name: format!("Audio {index}"),
            elements: Vec::new(),
            muted: false,
        });
    }

    let mut media_assets = Vec::new();
    for index in 0..shape.media_files {
        media_assets.push(json!({
            "id": format!("media-{index}"),
            "name": format!("shot-{index:04}.mp4"),
            "type": if index % 5 == 4 { "image" } else { "video" },
            "size": 48_000_000u64,
            "lastModified": 1_700_000_000_000i64,
            "width": 1920, "height": 1080,
            "duration": 12.0, "fps": 30.0, "hasAudio": true,
            "fileName": format!("shot-{index:04}.mp4")
        }));
    }

    for index in 0..shape.elements {
        let media = format!("media-{}", rng.below(shape.media_files.max(1)));
        let start = rng.unit() * (span - 12.0).max(0.0);
        let duration = 2.0 + rng.unit() * 8.0;
        let kind = index % 10;

        let element: Value = match kind {
            0..=3 => json!({
                "type": "video",
                "id": format!("element-{index}"),
                "name": format!("clip {index}"),
                "duration": seconds(duration),
                "startTime": seconds(start),
                "trimStart": 0, "trimEnd": 0,
                "sourceDuration": seconds(12.0),
                "mediaId": media,
                "volume": 1.0, "muted": false,
                "transform": transform(),
                "crop": { "left": 0.0, "top": 0.0, "right": 0.0, "bottom": 0.0 },
                "opacity": 1.0,
                "blendMode": "normal",
                "effects": effects(&mut rng, shape.effects_per_element),
                "masks": masks(&mut rng),
                "transition": transition(&mut rng),
                "animations": animations(&mut rng, start, duration, shape.keyframes_per_channel),
                "retime": { "rate": 1.0, "maintainPitch": true }
            }),
            4 | 5 => json!({
                "type": "text",
                "id": format!("element-{index}"),
                "name": format!("title {index}"),
                "duration": seconds(duration),
                "startTime": seconds(start),
                "trimStart": 0, "trimEnd": 0,
                "content": format!("Subtitle line {index} — the quick brown fox jumps over the lazy dog"),
                "fontSize": 48.0,
                "fontFamily": "Inter",
                "color": "#ffffff",
                "background": { "enabled": true, "color": "#00000080", "cornerRadius": 8.0, "paddingX": 12.0, "paddingY": 6.0 },
                "textAlign": "center",
                "fontWeight": "bold",
                "fontStyle": "normal",
                "textDecoration": "none",
                "transform": transform(),
                "opacity": 1.0,
                "effects": effects(&mut rng, 1),
                "animations": animations(&mut rng, start, duration, shape.keyframes_per_channel)
            }),
            6 => json!({
                "type": "image",
                "id": format!("element-{index}"),
                "name": format!("still {index}"),
                "duration": seconds(duration),
                "startTime": seconds(start),
                "trimStart": 0, "trimEnd": 0,
                "mediaId": media,
                "transform": transform(),
                "opacity": 1.0,
                "effects": effects(&mut rng, shape.effects_per_element),
                "masks": masks(&mut rng),
                "animations": animations(&mut rng, start, duration, shape.keyframes_per_channel)
            }),
            7 => json!({
                "type": "audio",
                "id": format!("element-{index}"),
                "name": format!("track {index}"),
                "duration": seconds(duration),
                "startTime": seconds(start),
                "trimStart": 0, "trimEnd": 0,
                "sourceType": "media",
                "mediaId": media,
                "volume": 0.8,
                "muted": false
            }),
            8 => json!({
                "type": "sticker",
                "id": format!("element-{index}"),
                "name": format!("sticker {index}"),
                "duration": seconds(duration),
                "startTime": seconds(start),
                "trimStart": 0, "trimEnd": 0,
                "stickerId": "shape-circle",
                "intrinsicWidth": 256.0, "intrinsicHeight": 256.0,
                "transform": transform(),
                "opacity": 1.0,
                "effects": effects(&mut rng, 1),
                "animations": animations(&mut rng, start, duration, shape.keyframes_per_channel)
            }),
            _ => json!({
                "type": "graphic",
                "id": format!("element-{index}"),
                "name": format!("graphic {index}"),
                "duration": seconds(duration),
                "startTime": seconds(start),
                "trimStart": 0, "trimEnd": 0,
                "definitionId": "lower-third",
                "params": { "title": format!("Speaker {index}"), "subtitle": "Role", "accent": "#ff8800" },
                "transform": transform(),
                "opacity": 1.0,
                "effects": effects(&mut rng, shape.effects_per_element),
                "masks": masks(&mut rng),
                "animations": animations(&mut rng, start, duration, shape.keyframes_per_channel)
            }),
        };

        let element: TimelineElement = serde_json::from_value(element)
            .unwrap_or_else(|error| panic!("element {index} kind {kind}: {error}"));

        let scene = project.scenes.first_mut().expect("scene");
        if matches!(element, TimelineElement::Audio(_)) {
            let slot = rng.below(scene.tracks.audio.len().max(1));
            scene.tracks.audio[slot].elements_mut().push(element);
        } else if kind <= 3 && rng.below(3) == 0 {
            scene.tracks.main.elements_mut().push(element);
        } else {
            let slot = rng.below(scene.tracks.overlay.len().max(1));
            scene.tracks.overlay[slot].elements_mut().push(element);
        }
    }

    project.metadata.duration = MediaTime::from_ticks(seconds(span));
    project
        .extra
        .insert("mediaItems".to_string(), Value::Array(media_assets));
    project
}
