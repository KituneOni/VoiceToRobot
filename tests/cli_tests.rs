use std::f32::consts::PI;

use assert_cmd::Command;
use hound::{SampleFormat, WavSpec, WavWriter};
use predicates::prelude::*;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// ヘルパー関数
// ---------------------------------------------------------------------------

/// 16-bit PCM モノラル WAV を生成する
fn create_wav_i16_mono(path: &std::path::Path, sample_rate: u32, samples: &[i16]) {
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec).unwrap();
    for &s in samples {
        writer.write_sample(s).unwrap();
    }
    writer.finalize().unwrap();
}

/// 16-bit PCM ステレオ WAV を生成する（インターリーブ済みサンプル）
fn create_wav_i16_stereo(path: &std::path::Path, sample_rate: u32, samples: &[i16]) {
    let spec = WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec).unwrap();
    for &s in samples {
        writer.write_sample(s).unwrap();
    }
    writer.finalize().unwrap();
}

/// 32-bit Float モノラル WAV を生成する
fn create_wav_f32_mono(path: &std::path::Path, sample_rate: u32, samples: &[f32]) {
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    let mut writer = WavWriter::create(path, spec).unwrap();
    for &s in samples {
        writer.write_sample(s).unwrap();
    }
    writer.finalize().unwrap();
}

/// 24-bit PCM モノラル WAV を生成する（非サポートフォーマットテスト用）
fn create_wav_i24_mono(path: &std::path::Path, sample_rate: u32, samples: &[i32]) {
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 24,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec).unwrap();
    for &s in samples {
        writer.write_sample(s).unwrap();
    }
    writer.finalize().unwrap();
}

/// 出力WAVを i16 サンプル列として読み出す
fn read_wav_i16(path: &std::path::Path) -> (WavSpec, Vec<i16>) {
    let reader = hound::WavReader::open(path).unwrap();
    let spec = reader.spec();
    let samples: Vec<i16> = reader.into_samples::<i16>().map(|s| s.unwrap()).collect();
    (spec, samples)
}

/// 出力WAVを f32 サンプル列として読み出す
fn read_wav_f32(path: &std::path::Path) -> (WavSpec, Vec<f32>) {
    let reader = hound::WavReader::open(path).unwrap();
    let spec = reader.spec();
    let samples: Vec<f32> = reader.into_samples::<f32>().map(|s| s.unwrap()).collect();
    (spec, samples)
}

fn cmd() -> Command {
    Command::cargo_bin("voice-to-robot").unwrap()
}

// ===========================================================================
// エラー系テスト
// ===========================================================================

#[test]
fn missing_input_file() {
    let dir = TempDir::new().unwrap();
    let output = dir.path().join("out.wav");

    cmd()
        .args(["nonexistent.wav", output.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("入力ファイルが見つかりません"));
}

#[test]
fn invalid_frequency_zero() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.wav");
    let output = dir.path().join("out.wav");
    create_wav_i16_mono(&input, 44100, &[0; 100]);

    cmd()
        .args([input.to_str().unwrap(), output.to_str().unwrap(), "-f", "0"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("正の値"));
}

#[test]
fn invalid_frequency_negative() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.wav");
    let output = dir.path().join("out.wav");
    create_wav_i16_mono(&input, 44100, &[0; 100]);

    cmd()
        .args([
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            "-f",
            "-10",
        ])
        .assert()
        .failure();
}

#[test]
fn frequency_above_nyquist() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.wav");
    let output = dir.path().join("out.wav");
    create_wav_i16_mono(&input, 44100, &[0; 100]);

    cmd()
        .args([
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            "-f",
            "25000",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ナイキスト周波数"));
}

#[test]
fn output_file_exists_without_yes() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.wav");
    let output = dir.path().join("out.wav");
    create_wav_i16_mono(&input, 44100, &[0; 100]);
    create_wav_i16_mono(&output, 44100, &[0; 10]); // 出力先に既存ファイルを配置

    cmd()
        .args([input.to_str().unwrap(), output.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("既に存在します"));
}

#[test]
fn unsupported_format_24bit() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.wav");
    let output = dir.path().join("out.wav");
    create_wav_i24_mono(&input, 44100, &[0; 100]);

    cmd()
        .args([input.to_str().unwrap(), output.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("サポートされていない"));
}

// ===========================================================================
// 正常系テスト
// ===========================================================================

#[test]
fn process_16bit_mono() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.wav");
    let output = dir.path().join("out.wav");

    let samples: Vec<i16> = (0..1000).map(|i| ((i % 200) * 100) as i16).collect();
    create_wav_i16_mono(&input, 44100, &samples);

    cmd()
        .args([
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            "-f",
            "100",
        ])
        .assert()
        .success();

    let (spec, out_samples) = read_wav_i16(&output);
    assert_eq!(spec.channels, 1);
    assert_eq!(spec.sample_rate, 44100);
    assert_eq!(spec.bits_per_sample, 16);
    assert_eq!(spec.sample_format, SampleFormat::Int);
    assert_eq!(out_samples.len(), samples.len());
}

#[test]
fn process_16bit_stereo() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.wav");
    let output = dir.path().join("out.wav");

    // L=10000, R=10000 を 100 フレーム分
    let mut samples = Vec::new();
    for _ in 0..100 {
        samples.push(10000i16); // L
        samples.push(10000i16); // R
    }
    create_wav_i16_stereo(&input, 44100, &samples);

    cmd()
        .args([
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            "-f",
            "100",
        ])
        .assert()
        .success();

    let (spec, out_samples) = read_wav_i16(&output);
    assert_eq!(spec.channels, 2);
    assert_eq!(out_samples.len(), 200);
}

#[test]
fn process_32bit_float() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.wav");
    let output = dir.path().join("out.wav");

    let samples: Vec<f32> = (0..1000).map(|i| (i as f32 / 1000.0) * 0.8).collect();
    create_wav_f32_mono(&input, 44100, &samples);

    cmd()
        .args([
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            "-f",
            "100",
        ])
        .assert()
        .success();

    let (spec, out_samples) = read_wav_f32(&output);
    assert_eq!(spec.sample_format, SampleFormat::Float);
    assert_eq!(spec.bits_per_sample, 32);
    assert_eq!(out_samples.len(), samples.len());

    // 全サンプルが [-1.0, 1.0] 範囲内であること
    for &s in &out_samples {
        assert!(
            (-1.0..=1.0).contains(&s),
            "サンプルが範囲外: {}",
            s
        );
    }
}

#[test]
fn output_overwrite_with_yes() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.wav");
    let output = dir.path().join("out.wav");
    create_wav_i16_mono(&input, 44100, &[1000; 100]);
    create_wav_i16_mono(&output, 44100, &[0; 10]); // 既存ファイル

    cmd()
        .args([input.to_str().unwrap(), output.to_str().unwrap(), "-y"])
        .assert()
        .success();

    // 上書きされたファイルのサンプル数が入力と同じであること
    let (_, out_samples) = read_wav_i16(&output);
    assert_eq!(out_samples.len(), 100);
}

#[test]
fn default_frequency() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.wav");
    let output = dir.path().join("out.wav");
    create_wav_i16_mono(&input, 44100, &[5000; 100]);

    // -f を指定しない → デフォルト 50Hz で処理
    cmd()
        .args([input.to_str().unwrap(), output.to_str().unwrap()])
        .assert()
        .success();

    let (_, out_samples) = read_wav_i16(&output);
    assert_eq!(out_samples.len(), 100);
}

// ===========================================================================
// DSP 正確性テスト
// ===========================================================================

#[test]
fn ring_mod_math_i16() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.wav");
    let output = dir.path().join("out.wav");

    let sample_rate: u32 = 44100;
    let freq: f32 = 100.0;
    let input_samples: Vec<i16> = vec![10000, 20000, 30000, -10000, -20000];
    create_wav_i16_mono(&input, sample_rate, &input_samples);

    cmd()
        .args([
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            "-f",
            &freq.to_string(),
        ])
        .assert()
        .success();

    let (_, out_samples) = read_wav_i16(&output);

    // 手計算で検証: y[n] = round(x[n] * sin(2π * freq * n / sample_rate))
    let angular_freq = 2.0 * PI * freq;
    for (n, (&x, &y)) in input_samples.iter().zip(out_samples.iter()).enumerate() {
        let mod_value = (angular_freq * n as f32 / sample_rate as f32).sin();
        let expected = (x as f32 * mod_value).round().clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        assert_eq!(
            y, expected,
            "フレーム {} のサンプルが不一致: got={}, expected={} (mod_value={})",
            n, y, expected, mod_value
        );
    }
}

#[test]
fn ring_mod_stereo_phase() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.wav");
    let output = dir.path().join("out.wav");

    let sample_rate: u32 = 44100;
    let freq: f32 = 200.0;

    // L と R に同じ値を入れる → 出力も L == R であるべき
    let mut interleaved = Vec::new();
    for _ in 0..50 {
        interleaved.push(15000i16); // L
        interleaved.push(15000i16); // R
    }
    create_wav_i16_stereo(&input, sample_rate, &interleaved);

    cmd()
        .args([
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            "-f",
            &freq.to_string(),
        ])
        .assert()
        .success();

    let (_, out_samples) = read_wav_i16(&output);

    // L/R が同一であることを検証（同じ入力 + 同じ mod_value = 同じ出力）
    for frame in 0..50 {
        let l = out_samples[frame * 2];
        let r = out_samples[frame * 2 + 1];
        assert_eq!(
            l, r,
            "フレーム {} で L({}) != R({}) — 位相がズレています",
            frame, l, r
        );
    }

    // さらに数式と一致するか検証
    let angular_freq = 2.0 * PI * freq;
    for frame in 0..50 {
        let mod_value = (angular_freq * frame as f32 / sample_rate as f32).sin();
        let expected = (15000.0_f32 * mod_value)
            .round()
            .clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        assert_eq!(
            out_samples[frame * 2],
            expected,
            "フレーム {} の値が数式と不一致",
            frame
        );
    }
}

#[test]
fn clipping_i16() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.wav");
    let output = dir.path().join("out.wav");

    // i16::MAX に近い値を入力し、sin値が1.0になるタイミングでもクリップしないことを確認
    // また、理論上オーバーフローしないことも確認
    let input_samples: Vec<i16> = vec![i16::MAX, i16::MIN, i16::MAX, i16::MIN];
    create_wav_i16_mono(&input, 44100, &input_samples);

    cmd()
        .args([
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            "-f",
            "100",
        ])
        .assert()
        .success();

    let (_, out_samples) = read_wav_i16(&output);

    // 出力が i16 範囲内に収まっていること（クラッシュしないこと自体がテスト）
    for &s in &out_samples {
        assert!(
            (i16::MIN..=i16::MAX).contains(&s),
            "出力サンプルが i16 範囲外: {}",
            s
        );
    }
}
