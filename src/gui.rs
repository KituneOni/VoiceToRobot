use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui;
use hound::{SampleFormat, WavReader};

use crate::audio::AudioPlayer;
use crate::dsp::{DspParams, Waveform};

/// GUIアプリケーションの状態
pub struct VoiceToRobotApp {
    /// 現在選択されているファイルパス
    file_path: Option<PathBuf>,
    /// DSPパラメータ（オーディオスレッドと共有）
    params: Arc<Mutex<DspParams>>,
    /// ローカルのパラメータコピー（UI描画用）
    local_params: DspParams,
    /// オーディオプレーヤー
    player: Option<AudioPlayer>,
    /// エラーメッセージ
    error_message: Option<String>,
    /// 読み込み済みファイル情報
    file_info: Option<FileInfo>,
}

struct FileInfo {
    sample_rate: u32,
    channels: u16,
    format: String,
    duration_secs: f32,
}

impl Default for VoiceToRobotApp {
    fn default() -> Self {
        let params = DspParams::default();
        Self {
            file_path: None,
            params: Arc::new(Mutex::new(params)),
            local_params: params,
            player: None,
            error_message: None,
            file_info: None,
        }
    }
}

impl VoiceToRobotApp {
    /// WAVファイルを読み込む
    fn load_file(&mut self, path: PathBuf) {
        self.stop_playback();
        self.error_message = None;
        self.file_info = None;

        match Self::read_wav_to_f32(&path) {
            Ok((samples, spec)) => {
                let total_frames = samples.len() / spec.channels as usize;
                let duration = total_frames as f32 / spec.sample_rate as f32;

                self.file_info = Some(FileInfo {
                    sample_rate: spec.sample_rate,
                    channels: spec.channels,
                    format: format!("{:?} {}-bit", spec.sample_format, spec.bits_per_sample),
                    duration_secs: duration,
                });

                self.player = Some(AudioPlayer::new(
                    samples,
                    spec.channels as usize,
                    spec.sample_rate,
                ));
                self.file_path = Some(path);
            }
            Err(e) => {
                self.error_message = Some(format!("{:#}", e));
                self.file_path = None;
            }
        }
    }

    /// WAVファイルを全サンプル f32 として読み込む（プレビュー用オンメモリ）
    fn read_wav_to_f32(
        path: &PathBuf,
    ) -> anyhow::Result<(Vec<f32>, hound::WavSpec)> {
        let reader = WavReader::open(path)
            .map_err(|e| anyhow::anyhow!("WAVファイルを開けません: {}", e))?;

        let spec = reader.spec();

        match (spec.sample_format, spec.bits_per_sample) {
            (SampleFormat::Int, 16) => {
                let samples: Vec<f32> = reader
                    .into_samples::<i16>()
                    .map(|s| s.map(|v| v as f32 / i16::MAX as f32))
                    .collect::<Result<Vec<f32>, _>>()
                    .map_err(|e| anyhow::anyhow!("サンプル読み込みエラー: {}", e))?;
                Ok((samples, spec))
            }
            (SampleFormat::Float, 32) => {
                let samples: Vec<f32> = reader
                    .into_samples::<f32>()
                    .collect::<Result<Vec<f32>, _>>()
                    .map_err(|e| anyhow::anyhow!("サンプル読み込みエラー: {}", e))?;
                Ok((samples, spec))
            }
            _ => Err(anyhow::anyhow!(
                "サポートされていないフォーマット: {:?} {}-bit\n対応: 16-bit PCM / 32-bit Float",
                spec.sample_format,
                spec.bits_per_sample
            )),
        }
    }

    /// パラメータを共有状態に同期する
    fn sync_params(&self) {
        if let Ok(mut p) = self.params.lock() {
            *p = self.local_params;
        }
    }

    /// 再生を停止する
    fn stop_playback(&mut self) {
        if let Some(player) = &mut self.player {
            player.stop();
            player.reset();
        }
    }

    /// エクスポート処理（CLIのストリーミング処理を流用）
    fn export(&mut self, output_path: PathBuf) {
        let Some(input_path) = &self.file_path else {
            self.error_message = Some("ファイルが選択されていません".to_string());
            return;
        };

        let reader = match WavReader::open(input_path) {
            Ok(r) => r,
            Err(e) => {
                self.error_message = Some(format!("入力ファイルを再度開けません: {}", e));
                return;
            }
        };

        let spec = reader.spec();
        let sample_rate = spec.sample_rate as f32;
        let channels = spec.channels as usize;
        let params = self.local_params;

        let writer = match hound::WavWriter::create(&output_path, spec) {
            Ok(w) => w,
            Err(e) => {
                self.error_message = Some(format!("出力ファイルを作成できません: {}", e));
                return;
            }
        };

        let result = match spec.sample_format {
            SampleFormat::Int => Self::export_i16(writer, reader, channels, sample_rate, &params),
            SampleFormat::Float => Self::export_f32(writer, reader, channels, sample_rate, &params),
        };

        match result {
            Ok(()) => self.error_message = None,
            Err(e) => self.error_message = Some(format!("エクスポートエラー: {:#}", e)),
        }
    }

    fn export_i16(
        mut writer: hound::WavWriter<std::io::BufWriter<std::fs::File>>,
        reader: WavReader<std::io::BufReader<std::fs::File>>,
        channels: usize,
        sample_rate: f32,
        params: &DspParams,
    ) -> anyhow::Result<()> {
        use crate::dsp;
        let mut samples_iter = reader.into_samples::<i16>();
        let mut frame_index: u64 = 0;

        loop {
            let phase = dsp::frame_phase(params.frequency, sample_rate, frame_index);
            let mod_value = dsp::generate_mod_value(params.waveform, phase);
            let mut done = false;
            for _ch in 0..channels {
                match samples_iter.next() {
                    Some(Ok(sample)) => {
                        let processed = dsp::apply_ring_mod(sample as f32, mod_value, params.mix);
                        let clamped =
                            processed.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16;
                        writer.write_sample(clamped)?;
                    }
                    _ => {
                        done = true;
                        break;
                    }
                }
            }
            if done {
                break;
            }
            frame_index += 1;
        }
        writer.finalize()?;
        Ok(())
    }

    fn export_f32(
        mut writer: hound::WavWriter<std::io::BufWriter<std::fs::File>>,
        reader: WavReader<std::io::BufReader<std::fs::File>>,
        channels: usize,
        sample_rate: f32,
        params: &DspParams,
    ) -> anyhow::Result<()> {
        use crate::dsp;
        let mut samples_iter = reader.into_samples::<f32>();
        let mut frame_index: u64 = 0;

        loop {
            let phase = dsp::frame_phase(params.frequency, sample_rate, frame_index);
            let mod_value = dsp::generate_mod_value(params.waveform, phase);
            let mut done = false;
            for _ch in 0..channels {
                match samples_iter.next() {
                    Some(Ok(sample)) => {
                        let processed =
                            dsp::apply_ring_mod(sample, mod_value, params.mix).clamp(-1.0, 1.0);
                        writer.write_sample(processed)?;
                    }
                    _ => {
                        done = true;
                        break;
                    }
                }
            }
            if done {
                break;
            }
            frame_index += 1;
        }
        writer.finalize()?;
        Ok(())
    }
}

impl eframe::App for VoiceToRobotApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("🤖 Voice to Robot");
            ui.separator();

            // --- ファイル選択 ---
            ui.horizontal(|ui| {
                if ui.button("📂 ファイルを開く").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("WAV", &["wav"])
                        .pick_file()
                    {
                        self.load_file(path);
                    }
                }
                if let Some(path) = &self.file_path {
                    ui.label(
                        path.file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default(),
                    );
                } else {
                    ui.label("ファイル未選択");
                }
            });

            // --- ファイル情報 ---
            if let Some(info) = &self.file_info {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "{}Hz / {}ch / {} / {:.1}秒",
                        info.sample_rate, info.channels, info.format, info.duration_secs
                    ));
                });
            }

            ui.separator();

            // --- パラメータ ---
            ui.heading("パラメータ");

            let mut changed = false;

            ui.horizontal(|ui| {
                ui.label("周波数:");
                if ui
                    .add(egui::Slider::new(&mut self.local_params.frequency, 10.0..=1000.0).suffix(" Hz"))
                    .changed()
                {
                    changed = true;
                }
            });

            ui.horizontal(|ui| {
                ui.label("波形:");
                for wf in [Waveform::Sine, Waveform::Square, Waveform::Saw, Waveform::Triangle] {
                    if ui
                        .radio_value(&mut self.local_params.waveform, wf, format!("{}", wf))
                        .changed()
                    {
                        changed = true;
                    }
                }
            });

            ui.horizontal(|ui| {
                ui.label("ミックス:");
                if ui
                    .add(egui::Slider::new(&mut self.local_params.mix, 0.0..=1.0))
                    .changed()
                {
                    changed = true;
                }
            });

            if changed {
                self.sync_params();
            }

            ui.separator();

            // --- 再生/停止・エクスポート ---
            let has_file = self.player.is_some();
            let is_playing = self
                .player
                .as_ref()
                .map(|p| p.is_playing.load(std::sync::atomic::Ordering::SeqCst))
                .unwrap_or(false);

            ui.horizontal(|ui| {
                ui.set_enabled(has_file);

                if is_playing {
                    if ui.button("⏹ 停止").clicked() {
                        self.stop_playback();
                    }
                } else if ui.button("▶ 再生").clicked() {
                    if let Some(player) = &mut self.player {
                        player.reset();
                        if let Err(e) = player.play(Arc::clone(&self.params)) {
                            self.error_message = Some(format!("再生エラー: {:#}", e));
                        }
                    }
                }

                if ui.button("💾 エクスポート").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("WAV", &["wav"])
                        .save_file()
                    {
                        self.export(path);
                    }
                }
            });

            // --- エラー表示 ---
            if let Some(err) = &self.error_message {
                ui.separator();
                ui.colored_label(egui::Color32::RED, format!("⚠ {}", err));
            }
        });

        // 再生中はUIを定期更新する（ボタン状態の反映）
        if self
            .player
            .as_ref()
            .map(|p| p.is_playing.load(std::sync::atomic::Ordering::SeqCst))
            .unwrap_or(false)
        {
            ctx.request_repaint();
        }
    }
}

/// GUIモードのエントリポイント
pub fn run_gui() -> anyhow::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([480.0, 400.0])
            .with_min_inner_size([400.0, 350.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Voice to Robot",
        options,
        Box::new(|_cc| Ok(Box::new(VoiceToRobotApp::default()))),
    )
    .map_err(|e| anyhow::anyhow!("GUIの起動に失敗しました: {}", e))
}
