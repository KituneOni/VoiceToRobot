use std::f32::consts::PI;

/// キャリア波形の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waveform {
    Sine,
    Square,
    Saw,
    Triangle,
}

impl std::fmt::Display for Waveform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Waveform::Sine => write!(f, "sine"),
            Waveform::Square => write!(f, "square"),
            Waveform::Saw => write!(f, "saw"),
            Waveform::Triangle => write!(f, "triangle"),
        }
    }
}

/// DSPパラメータ（GUI/CLI共用）
#[derive(Debug, Clone, Copy)]
pub struct DspParams {
    pub frequency: f32,
    pub waveform: Waveform,
    pub mix: f32,
}

impl Default for DspParams {
    fn default() -> Self {
        Self {
            frequency: 50.0,
            waveform: Waveform::Sine,
            mix: 1.0,
        }
    }
}

/// 波形の mod_value を計算する。
///
/// `phase` は累積位相（ラジアン）。
/// 正規化位相 p = (phase mod 2π) / 2π として各波形を計算する。
pub fn generate_mod_value(waveform: Waveform, phase: f32) -> f32 {
    match waveform {
        Waveform::Sine => phase.sin(),
        Waveform::Square => {
            let p = normalized_phase(phase);
            if p < 0.5 { 1.0 } else { -1.0 }
        }
        Waveform::Saw => {
            let p = normalized_phase(phase);
            2.0 * p - 1.0
        }
        Waveform::Triangle => {
            let p = normalized_phase(phase);
            2.0 * (2.0 * p - 1.0).abs() - 1.0
        }
    }
}

/// 1サンプルにリングモジュレーション + ドライ/ウェットミックスを適用する。
///
/// `output = sample * (1.0 - mix) + sample * mod_value * mix`
pub fn apply_ring_mod(sample: f32, mod_value: f32, mix: f32) -> f32 {
    sample * (1.0 - mix) + sample * mod_value * mix
}

/// フレームインデックスから位相（ラジアン）を計算する。
pub fn frame_phase(frequency: f32, sample_rate: f32, frame_index: u64) -> f32 {
    2.0 * PI * frequency * frame_index as f32 / sample_rate
}

/// 位相を 0.0..1.0 に正規化する
fn normalized_phase(phase: f32) -> f32 {
    let two_pi = 2.0 * PI;
    let p = (phase % two_pi) / two_pi;
    if p < 0.0 { p + 1.0 } else { p }
}

// ===========================================================================
// 単体テスト
// ===========================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    const EPSILON: f32 = 1e-6;

    // --- 波形生成テスト ---

    #[test]
    fn waveform_sine() {
        // sin(0) = 0, sin(π/2) = 1, sin(π) = 0, sin(3π/2) = -1
        assert!((generate_mod_value(Waveform::Sine, 0.0)).abs() < EPSILON);
        assert!((generate_mod_value(Waveform::Sine, PI / 2.0) - 1.0).abs() < EPSILON);
        assert!((generate_mod_value(Waveform::Sine, PI)).abs() < EPSILON);
        assert!((generate_mod_value(Waveform::Sine, 3.0 * PI / 2.0) + 1.0).abs() < EPSILON);
    }

    #[test]
    fn waveform_square() {
        // p < 0.5 → 1.0, p >= 0.5 → -1.0
        // θ=0 → p=0 → 1.0
        assert_eq!(generate_mod_value(Waveform::Square, 0.0), 1.0);
        // θ=π/2 → p=0.25 → 1.0
        assert_eq!(generate_mod_value(Waveform::Square, PI / 2.0), 1.0);
        // θ=π → p=0.5 → -1.0
        assert_eq!(generate_mod_value(Waveform::Square, PI), -1.0);
        // θ=3π/2 → p=0.75 → -1.0
        assert_eq!(generate_mod_value(Waveform::Square, 3.0 * PI / 2.0), -1.0);
    }

    #[test]
    fn waveform_saw() {
        // 2p - 1: p=0→-1, p=0.25→-0.5, p=0.5→0, p=0.75→0.5
        assert!((generate_mod_value(Waveform::Saw, 0.0) - (-1.0)).abs() < EPSILON);
        assert!((generate_mod_value(Waveform::Saw, PI / 2.0) - (-0.5)).abs() < EPSILON);
        assert!((generate_mod_value(Waveform::Saw, PI) - 0.0).abs() < EPSILON);
        assert!((generate_mod_value(Waveform::Saw, 3.0 * PI / 2.0) - 0.5).abs() < EPSILON);
    }

    #[test]
    fn waveform_triangle() {
        // 2|2p-1|-1: p=0→1, p=0.25→0, p=0.5→-1, p=0.75→0
        assert!((generate_mod_value(Waveform::Triangle, 0.0) - 1.0).abs() < EPSILON);
        assert!((generate_mod_value(Waveform::Triangle, PI / 2.0) - 0.0).abs() < EPSILON);
        assert!((generate_mod_value(Waveform::Triangle, PI) - (-1.0)).abs() < EPSILON);
        assert!((generate_mod_value(Waveform::Triangle, 3.0 * PI / 2.0) - 0.0).abs() < EPSILON);
    }

    #[test]
    fn waveform_values_at_boundaries() {
        // 2π でも位相が一巡して同じ値になることを確認
        let two_pi = 2.0 * PI;
        for wf in [Waveform::Sine, Waveform::Square, Waveform::Saw, Waveform::Triangle] {
            let at_zero = generate_mod_value(wf, 0.0);
            let at_two_pi = generate_mod_value(wf, two_pi);
            assert!(
                (at_zero - at_two_pi).abs() < 1e-4,
                "{:?}: θ=0 ({}) と θ=2π ({}) が不一致",
                wf, at_zero, at_two_pi
            );
        }
    }

    // --- リングモジュレーション + ミックス テスト ---

    #[test]
    fn apply_mod_sine_mix_full() {
        // mix=1.0: output = sample * mod_value
        let sample = 0.8;
        let mod_value = 0.5;
        let result = apply_ring_mod(sample, mod_value, 1.0);
        assert!((result - 0.4).abs() < EPSILON); // 0.8 * 0.5 = 0.4
    }

    #[test]
    fn apply_mod_mix_zero() {
        // mix=0.0: output = sample（原音がそのまま）
        let sample = 0.75;
        let mod_value = -1.0; // 極端な値でも関係ない
        let result = apply_ring_mod(sample, mod_value, 0.0);
        assert!((result - 0.75).abs() < EPSILON);
    }

    #[test]
    fn apply_mod_mix_half() {
        // mix=0.5: output = sample * 0.5 + sample * mod_value * 0.5
        let sample = 1.0;
        let mod_value = -1.0;
        let result = apply_ring_mod(sample, mod_value, 0.5);
        // 1.0 * 0.5 + 1.0 * (-1.0) * 0.5 = 0.5 - 0.5 = 0.0
        assert!((result - 0.0).abs() < EPSILON);
    }

    #[test]
    fn apply_mod_square_wave() {
        // Square波 θ=0 → mod_value=1.0 → mix=1: output = sample * 1.0
        let mod_value = generate_mod_value(Waveform::Square, 0.0);
        assert_eq!(mod_value, 1.0);
        let result = apply_ring_mod(0.6, mod_value, 1.0);
        assert!((result - 0.6).abs() < EPSILON);

        // Square波 θ=π → mod_value=-1.0 → mix=1: output = sample * -1.0
        let mod_value = generate_mod_value(Waveform::Square, PI);
        assert_eq!(mod_value, -1.0);
        let result = apply_ring_mod(0.6, mod_value, 1.0);
        assert!((result - (-0.6)).abs() < EPSILON);
    }

    // --- frame_phase テスト ---

    #[test]
    fn frame_phase_calculation() {
        // frame_index=0 → phase=0
        assert_eq!(frame_phase(100.0, 44100.0, 0), 0.0);

        // frame_index=441 → phase = 2π * 100 * 441 / 44100 = 2π
        let phase = frame_phase(100.0, 44100.0, 441);
        assert!((phase - 2.0 * PI).abs() < EPSILON);
    }
}
