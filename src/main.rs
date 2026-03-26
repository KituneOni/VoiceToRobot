use std::f32::consts::PI;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Parser;
use hound::{SampleFormat, WavReader, WavWriter};

/// WAVファイルにリングモジュレーションを適用し、ロボット風の音声に変換するCLIツール
#[derive(Parser)]
#[command(name = "voice-to-robot", version, about)]
struct Cli {
    /// 処理対象の入力WAVファイルパス
    input_path: PathBuf,

    /// 出力WAVファイルパス
    output_path: PathBuf,

    /// キャリア周波数 (Hz)
    #[arg(short = 'f', long = "frequency", default_value_t = 50.0)]
    frequency: f32,

    /// 出力先の既存ファイルを確認なしで上書きする
    #[arg(short = 'y', long = "yes")]
    overwrite: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // --- 入力ファイル存在確認 ---
    if !cli.input_path.exists() {
        bail!(
            "入力ファイルが見つかりません: {}",
            cli.input_path.display()
        );
    }

    // --- 出力先上書きチェック ---
    if cli.output_path.exists() && !cli.overwrite {
        bail!(
            "出力ファイルが既に存在します: {}\n上書きするには -y オプションを指定してください。",
            cli.output_path.display()
        );
    }

    // --- WAVファイル読み込み ---
    let reader = WavReader::open(&cli.input_path)
        .with_context(|| format!("WAVファイルを開けません: {}", cli.input_path.display()))?;

    let spec = reader.spec();
    let sample_rate = spec.sample_rate as f32;
    let channels = spec.channels as usize;

    // --- フォーマット検証 ---
    match (spec.sample_format, spec.bits_per_sample) {
        (SampleFormat::Int, 16) => {} // 16-bit PCM: OK
        (SampleFormat::Float, 32) => {} // 32-bit Float: OK
        _ => {
            bail!(
                "サポートされていないWAVフォーマットです: {:?} {}-bit\n\
                 対応フォーマット: 16-bit PCM または 32-bit Float",
                spec.sample_format,
                spec.bits_per_sample
            );
        }
    }

    // --- キャリア周波数バリデーション ---
    let nyquist = sample_rate / 2.0;
    if cli.frequency <= 0.0 {
        bail!(
            "キャリア周波数は正の値である必要があります: {} Hz",
            cli.frequency
        );
    }
    if cli.frequency >= nyquist {
        bail!(
            "キャリア周波数がナイキスト周波数を超えています: {} Hz (上限: {} Hz)",
            cli.frequency,
            nyquist
        );
    }

    // --- 出力WAVライター作成 ---
    let mut writer = WavWriter::create(&cli.output_path, spec)
        .with_context(|| format!("出力ファイルを作成できません: {}", cli.output_path.display()))?;

    // --- リングモジュレーション処理（ストリーミング） ---
    let angular_freq = 2.0 * PI * cli.frequency;

    match spec.sample_format {
        SampleFormat::Int => {
            process_int16(&mut writer, reader, channels, angular_freq, sample_rate)?;
        }
        SampleFormat::Float => {
            process_float32(&mut writer, reader, channels, angular_freq, sample_rate)?;
        }
    }

    writer
        .finalize()
        .context("WAVファイルの書き込みを完了できません")?;

    eprintln!(
        "完了: {} -> {} (キャリア周波数: {} Hz)",
        cli.input_path.display(),
        cli.output_path.display(),
        cli.frequency
    );

    Ok(())
}

/// 16-bit PCM のリングモジュレーション処理
fn process_int16(
    writer: &mut WavWriter<std::io::BufWriter<std::fs::File>>,
    reader: WavReader<std::io::BufReader<std::fs::File>>,
    channels: usize,
    angular_freq: f32,
    sample_rate: f32,
) -> Result<()> {
    let mut samples_iter = reader.into_samples::<i16>();
    let mut frame_index: u64 = 0;

    loop {
        // 1フレーム分のサンプルを読み込む
        let mod_value = (angular_freq * frame_index as f32 / sample_rate).sin();

        let mut frame_complete = true;
        for _ch in 0..channels {
            match samples_iter.next() {
                Some(sample_result) => {
                    let sample = sample_result.context("サンプルの読み込みに失敗しました")?;
                    let processed = (sample as f32 * mod_value).round();
                    let clamped = processed.clamp(i16::MIN as f32, i16::MAX as f32) as i16;
                    writer
                        .write_sample(clamped)
                        .context("サンプルの書き込みに失敗しました")?;
                }
                None => {
                    frame_complete = false;
                    break;
                }
            }
        }

        if !frame_complete {
            break;
        }

        frame_index += 1;
    }

    Ok(())
}

/// 32-bit Float のリングモジュレーション処理
fn process_float32(
    writer: &mut WavWriter<std::io::BufWriter<std::fs::File>>,
    reader: WavReader<std::io::BufReader<std::fs::File>>,
    channels: usize,
    angular_freq: f32,
    sample_rate: f32,
) -> Result<()> {
    let mut samples_iter = reader.into_samples::<f32>();
    let mut frame_index: u64 = 0;

    loop {
        let mod_value = (angular_freq * frame_index as f32 / sample_rate).sin();

        let mut frame_complete = true;
        for _ch in 0..channels {
            match samples_iter.next() {
                Some(sample_result) => {
                    let sample = sample_result.context("サンプルの読み込みに失敗しました")?;
                    let processed = (sample * mod_value).clamp(-1.0, 1.0);
                    writer
                        .write_sample(processed)
                        .context("サンプルの書き込みに失敗しました")?;
                }
                None => {
                    frame_complete = false;
                    break;
                }
            }
        }

        if !frame_complete {
            break;
        }

        frame_index += 1;
    }

    Ok(())
}
