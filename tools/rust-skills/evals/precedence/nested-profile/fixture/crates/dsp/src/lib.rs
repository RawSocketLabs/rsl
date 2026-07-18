pub fn scale(samples: &mut [f32], factor: f32) {
    for sample in samples {
        *sample *= factor;
    }
}
