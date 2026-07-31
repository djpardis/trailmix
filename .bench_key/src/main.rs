use std::time::Instant;
fn main() {
    let sr = 44100u32;
    for secs in [20u32, 240] {
        let samples: Vec<f32> = (0..sr * secs)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr as f32).sin() * 0.25)
            .collect();

        // Key Lime
        let config = key_lime::KeyConfig::default();
        let _ = key_lime::analyze(&samples, sr, config);
        let start = Instant::now();
        for _ in 0..5 { let _ = key_lime::analyze(&samples, sr, config); }
        println!("key  {}s: {:.1} ms", secs, start.elapsed().as_secs_f64() * 200.0);

        // Beat Salad
        let bconfig = beat_salad::BeatConfig::default();
        let _ = beat_salad::analyze(&samples, sr, bconfig);
        let start = Instant::now();
        for _ in 0..5 { let _ = beat_salad::analyze(&samples, sr, bconfig); }
        println!("beat {}s: {:.1} ms", secs, start.elapsed().as_secs_f64() * 200.0);
    }
}
