fn main() {
    println!("{}", video::ffmpeg::status());
    println!("decode: {}", video::ffmpeg::can_decode());
    println!("encode api: {}", video::ffmpeg::can_encode());
    println!(
        "h264 encoders: {:?}",
        video::ffmpeg::available_h264_encoders()
    );
    println!("best: {:?}", video::ffmpeg::best_h264_encoder());

    let Some(name) = video::ffmpeg::best_h264_encoder() else {
        return;
    };
    let (width, height) = (640usize, 360usize);
    let mut encoder =
        video::ffmpeg::H264Encoder::open(name, width as u32, height as u32, (30, 1), 4_000_000)
            .expect("encoder");

    let mut rgba = vec![0u8; width * height * 4];
    let mut y = vec![0u8; width * height];
    let mut u = vec![0u8; width * height / 4];
    let mut v = vec![0u8; width * height / 4];

    let mut packets = 0usize;
    let mut keyframes = 0usize;
    let mut bytes = 0usize;
    for frame in 0..60u32 {
        for row in 0..height {
            for column in 0..width {
                let base = (row * width + column) * 4;
                rgba[base] = ((column + frame as usize * 3) % 256) as u8;
                rgba[base + 1] = ((row + frame as usize) % 256) as u8;
                rgba[base + 2] = (frame * 4) as u8;
                rgba[base + 3] = 255;
            }
        }
        video::color::rgba_to_i420(&rgba, (width, height), &mut y, &mut u, &mut v);
        for packet in encoder.encode(&y, &u, &v).expect("encode") {
            packets += 1;
            bytes += packet.bytes.len();
            keyframes += usize::from(packet.is_sync);
        }
    }
    for packet in encoder.finish().expect("flush") {
        packets += 1;
        bytes += packet.bytes.len();
        keyframes += usize::from(packet.is_sync);
    }
    println!("packets: {packets}, keyframes: {keyframes}, bytes: {bytes}");
}
