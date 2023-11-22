use std::f32::consts::PI;

struct Equalizer {
    bands: Vec<Band>,
    sample_rate: f32,
    sin_lookup: Vec<f32>,
    cos_lookup: Vec<f32>,
}

struct Band {
    gain: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Equalizer {
    fn new(sample_rate: f32) -> Self {
        Equalizer {
            bands: Vec::new(),
            sample_rate,
            sin_lookup: generate_sin_lookup(),
            cos_lookup: generate_cos_lookup(),
        }
    }

    fn add_band(&mut self, gain: f32) {
        // Initialize a new band and add it to the vector
        let band = Band {
            gain,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        };
        self.bands.push(band);
    }

    fn remove_band(&mut self, index: usize) {
        // Remove a band at the specified index
        if index < self.bands.len() {
            self.bands.remove(index);
        }
    }

    fn process_sample(&mut self, sample: f32) -> f32 {
        let mut output = sample;

        for band in &mut self.bands {
            // Apply equalization to the sample using the band gain
            output = self.apply_band_gain(output, band);
        }

        output
    }

    fn apply_band_gain(&self, sample: f32, band: &Band) -> f32 {
        // Implement your 5-band equalizer here
        // For simplicity, we'll use a simple first-order filter (biquad) for each band
        let frequency = 1000.0; // Adjust as needed

        let q = 1.0; // Q factor for the filter
        let omega = 2.0 * PI * frequency / self.sample_rate;
        let alpha = (omega / (2.0 * q)).sin() / (1.0 + (omega / (2.0 * q)).cos());

        let sin_omega = self.sin_lookup[(omega * 256.0 / (2.0 * PI)) as usize];
        let cos_omega = self.cos_lookup[(omega * 256.0 / (2.0 * PI)) as usize];

        let b0 = 1.0 + alpha * band.gain;
        let b1 = -2.0 * cos_omega;
        let b2 = 1.0 - alpha * band.gain;
        let a0 = 1.0 + alpha / band.gain;
        let a1 = -2.0 * cos_omega;
        let a2 = 1.0 - alpha / band.gain;

        let new_sample = (
            b0 * sample + 
            b1 * band.x1 + 
            b2 * band.x2 - 
            a1 * band.y1 - 
            a2 * band.y2 ) / a0;

        // Update state variables for the next iteration
        band.x2 = band.x1;
        band.x1 = sample;
        band.y2 = band.y1;
        band.y1 = new_sample;

        new_sample
    }
}

fn generate_sin_lookup() -> Vec<f32> {
    (0..256)
        .map(|i| (i as f32 * 2.0 * PI / 256.0).sin())
        .collect()
}

fn generate_cos_lookup() -> Vec<f32> {
    (0..256)
        .map(|i| (i as f32 * 2.0 * PI / 256.0).cos())
        .collect()
}
/*
fn main() {
    // Set your sample rate (e.g., 44100.0 Hz)
    let sample_rate = 44100.0;

    // Initialize the equalizer
    let mut equalizer = Equalizer::new(sample_rate);

    // Add some bands (adjust gains as needed)
    equalizer.add_band(1.0);
    equalizer.add_band(1.0);
    equalizer.add_band(1.0);

    // Example usage:
    let input_sample = 0.5; // Replace with your input sample
    let output_sample = equalizer.process_sample(input_sample);

    // Print the result
    println!("Input: {:.4}, Output: {:.4}", input_sample, output_sample);
}
*/