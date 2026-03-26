use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Parser;
use hound::{SampleFormat, WavReader, WavWriter};

use crate::dsp::{self, DspParams, Waveform};

/// WAVファイルにリングモジュレーションを適用し、ロボット風の音声に変換するCLIツール
#[derive(Parser)]
#[command(name = "voice-to-robot", version, about)]
pub struct CliArgs {
    /// 処理対象の入力WAVファイルパス
    input_path: PathBuf,

    /// 出力WAVファイルパス
    output_path: PathBuf,

    /// キャリア周波数 (Hz)
    #[arg(short = 'f', long = "frequency", default_value_t = 50.0)]
    frequency: f32,

    /// 波形の種類 (sine, square, saw, triangle)
    #[arg(short = 'w', long = "waveform", default_value = "sine")]
    waveform: String,

    /// ドライ/ウェット ミックス (0.0=原音, 1.0=加工音100%)
    #[arg(short = 'm', long = "mix", default_value_t = 1.0)]
    mix: f32,

    /// 出力先の既存ファイルを確認なしで上書きする
    #[arg(short = 'y', long = "yes")]
    overwrite: bool,
}

/// CLIモードのエントリポイント
pub fn run_cli() -> Result<()> {
    let args = CliArgs::parse();

    // --- 波形パース ---
    let waveform = match args.waveform.as_str() {
        "sine" => Waveform::Sine,
        "square" => Waveform::Square,
        "saw" => Waveform::Saw,
        "triangle" => Waveform::Triangle,
        other => bail!("無効な波形です: '{}'\n有効な値: sine, square, saw, triangle", other),
    };

    // --- ミックスバリデーション ---
    if !(0.0..=1.0).contains(&args.mix) {
        bail!(
            "ミックス値は 0.0 から 1.0 の範囲で指定してください: {}",
            args.mix
        );
    }

    // --- 入力ファイル存在確認 ---
    if !args.input_path.exists() {
        bail!(
            "入力ファイルが見つかりません: {}",
            args.input_path.display()
        );
    }

    // --- 出力先上書きチェック ---
    if args.output_path.exists() && !args.overwrite {
        bail!(
            "出力ファイルが既に存在します: {}\n上書きするには -y オプションを指定してください。",
            args.output_path.display()
        );
    }

    // --- WAVファイル読み込み ---
    let reader = WavReader::open(&args.input_path)
        .with_context(|| format!("WAVファイルを開けません: {}", args.input_path.display()))?;

    let spec = reader.spec();
    let sample_rate = spec.sample_rate as f32;
    let channels = spec.channels as usize;

    // --- フォーマット検証 ---
    match (spec.sample_format, spec.bits_per_sample) {
        (SampleFormat::Int, 16) => {}
        (SampleFormat::Float, 32) => {}
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
    if args.frequency <= 0.0 {
        bail!(
            "キャリア周波数は正の値である必要があります: {} Hz",
            args.frequency
        );
    }
    if args.frequency >= nyquist {
        bail!(
            "キャリア周波数がナイキスト周波数を超えています: {} Hz (上限: {} Hz)",
            args.frequency,
            nyquist
        );
    }

    let params = DspParams {
        frequency: args.frequency,
        waveform,
        mix: args.mix,
    };

    // --- 出力WAVライター作成 ---
    let mut writer = WavWriter::create(&args.output_path, spec)
        .with_context(|| format!("出力ファイルを作成できません: {}", args.output_path.display()))?;

    // --- リングモジュレーション処理（ストリーミング） ---
    match spec.sample_format {
        SampleFormat::Int => {
            process_streaming_i16(&mut writer, reader, channels, sample_rate, &params)?;
        }
        SampleFormat::Float => {
            process_streaming_f32(&mut writer, reader, channels, sample_rate, &params)?;
        }
    }

    writer
        .finalize()
        .context("WAVファイルの書き込みを完了できません")?;

    eprintln!(
        "完了: {} -> {} (周波数: {} Hz, 波形: {}, ミックス: {})",
        args.input_path.display(),
        args.output_path.display(),
        args.frequency,
        args.waveform,
        args.mix,
    );

    Ok(())
}

/// 16-bit PCM ストリーミング処理
fn process_streaming_i16(
    writer: &mut WavWriter<std::io::BufWriter<std::fs::File>>,
    reader: WavReader<std::io::BufReader<std::fs::File>>,
    channels: usize,
    sample_rate: f32,
    params: &DspParams,
) -> Result<()> {
    let mut samples_iter = reader.into_samples::<i16>();
    let mut frame_index: u64 = 0;

    loop {
        let phase = dsp::frame_phase(params.frequency, sample_rate, frame_index);
        let mod_value = dsp::generate_mod_value(params.waveform, phase);

        let mut frame_complete = true;
        for _ch in 0..channels {
            match samples_iter.next() {
                Some(sample_result) => {
                    let sample = sample_result.context("サンプルの読み込みに失敗しました")?;
                    let processed = dsp::apply_ring_mod(sample as f32, mod_value, params.mix);
                    let clamped = processed.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16;
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

/// 32-bit Float ストリーミング処理
fn process_streaming_f32(
    writer: &mut WavWriter<std::io::BufWriter<std::fs::File>>,
    reader: WavReader<std::io::BufReader<std::fs::File>>,
    channels: usize,
    sample_rate: f32,
    params: &DspParams,
) -> Result<()> {
    let mut samples_iter = reader.into_samples::<f32>();
    let mut frame_index: u64 = 0;

    loop {
        let phase = dsp::frame_phase(params.frequency, sample_rate, frame_index);
        let mod_value = dsp::generate_mod_value(params.waveform, phase);

        let mut frame_complete = true;
        for _ch in 0..channels {
            match samples_iter.next() {
                Some(sample_result) => {
                    let sample = sample_result.context("サンプルの読み込みに失敗しました")?;
                    let processed = dsp::apply_ring_mod(sample, mod_value, params.mix).clamp(-1.0, 1.0);
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
