use std::f64::consts::PI;

pub const ANALYSIS_WINDOW: usize = 2048;

pub fn analyze(samples: &[f32], rate: u32, bands: usize) -> Vec<f32> {
    if bands == 0 || samples.len() < ANALYSIS_WINDOW {
        return vec![0.0; bands];
    }
    let n = ANALYSIS_WINDOW;
    let start = samples.len() - n;
    let mut real = vec![0.0f64; n];
    let mut imaginary = vec![0.0f64; n];
    for i in 0..n {
        let hanning = 0.5 * (1.0 - (2.0 * PI * i as f64 / (n as f64 - 1.0)).cos());
        real[i] = samples[start + i] as f64 * hanning;
    }
    fft(&mut real, &mut imaginary);

    let nyquist_bin = n / 2;
    let bin_hz = rate as f64 / n as f64;
    let min_hz = 50.0f64;
    let max_hz = (rate as f64 / 2.0).min(16_000.0);
    let mut output = vec![0.0f32; bands];

    for (band, slot) in output.iter_mut().enumerate() {
        let low_ratio = band as f64 / bands as f64;
        let high_ratio = (band + 1) as f64 / bands as f64;
        let low = min_hz * (max_hz / min_hz).powf(low_ratio);
        let high = min_hz * (max_hz / min_hz).powf(high_ratio);
        let first = ((low / bin_hz).floor() as usize).max(1);
        let last = ((high / bin_hz).ceil() as usize).clamp(first + 1, nyquist_bin);

        let mut magnitude: f64 = 0.0;
        for bin in first..last {
            magnitude = magnitude.max(real[bin].hypot(imaginary[bin]));
        }
        let normalized = magnitude / (n as f64 / 2.0);
        let db = 20.0 * normalized.max(1e-9).log10();
        let tilt = 6.0 * low_ratio;
        *slot = ((db + 56.0 + tilt) / 58.0).clamp(0.0, 1.0) as f32;
    }
    output
}

fn fft(real: &mut [f64], imaginary: &mut [f64]) {
    let n = real.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            real.swap(i, j);
            imaginary.swap(i, j);
        }
    }

    let mut len = 2;
    while len <= n {
        let angle = -2.0 * PI / len as f64;
        let (twiddle_real, twiddle_imaginary) = (angle.cos(), angle.sin());
        let half = len / 2;
        for chunk in (0..n).step_by(len) {
            let (mut wr, mut wi) = (1.0, 0.0);
            for k in 0..half {
                let even = chunk + k;
                let odd = even + half;
                let odd_real = real[odd] * wr - imaginary[odd] * wi;
                let odd_imaginary = real[odd] * wi + imaginary[odd] * wr;
                let even_real = real[even];
                let even_imaginary = imaginary[even];
                real[even] = even_real + odd_real;
                imaginary[even] = even_imaginary + odd_imaginary;
                real[odd] = even_real - odd_real;
                imaginary[odd] = even_imaginary - odd_imaginary;
                let next_wr = wr * twiddle_real - wi * twiddle_imaginary;
                wi = wr * twiddle_imaginary + wi * twiddle_real;
                wr = next_wr;
            }
        }
        len <<= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    #[test]
    fn silence_is_flat() {
        let bands = analyze(&vec![0.0; ANALYSIS_WINDOW], 44_100, 32);
        assert!(bands.iter().all(|value| *value == 0.0));
    }

    #[test]
    fn tone_peaks_in_its_band() {
        let rate = 44_100u32;
        let samples: Vec<f32> = (0..ANALYSIS_WINDOW)
            .map(|i| (TAU * 440.0 * i as f64 / rate as f64).sin() as f32)
            .collect();
        let bands = analyze(&samples, rate, 48);
        let loudest = bands
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(index, _)| index)
            .unwrap();
        let low = 50.0 * (16_000.0f64 / 50.0).powf(loudest as f64 / 48.0);
        let high = 50.0 * (16_000.0f64 / 50.0).powf((loudest + 1) as f64 / 48.0);
        assert!(
            low <= 440.0 && 440.0 <= high,
            "440 Hz landed in {low}..{high}"
        );
    }
}
