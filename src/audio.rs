use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::dsp::{self, DspParams};

/// オーディオプレイバックの状態を管理する
pub struct AudioPlayer {
    /// cpal ストリーム（Some の間は再生中）
    stream: Option<cpal::Stream>,
    /// 再生中フラグ
    pub is_playing: Arc<AtomicBool>,
    /// 現在の再生位置（ソースデータのフレーム単位、f64で小数部も保持）
    /// リサンプリング時の補間精度のために f64 をビット表現で格納する
    play_position_bits: Arc<AtomicU64>,
    /// 累積位相インデックス（ループ時もリセットしない、ソースレート基準）
    phase_counter_bits: Arc<AtomicU64>,
    /// オーディオデータ（f32, インターリーブ、ソースのチャンネル数）
    audio_data: Arc<Vec<f32>>,
    /// ソースのチャンネル数
    src_channels: usize,
    /// ソースのサンプルレート
    pub sample_rate: u32,
}

impl AudioPlayer {
    /// 新しい AudioPlayer を作成する（まだ再生しない）
    pub fn new(audio_data: Vec<f32>, channels: usize, sample_rate: u32) -> Self {
        Self {
            stream: None,
            is_playing: Arc::new(AtomicBool::new(false)),
            play_position_bits: Arc::new(AtomicU64::new(0.0_f64.to_bits())),
            phase_counter_bits: Arc::new(AtomicU64::new(0.0_f64.to_bits())),
            audio_data: Arc::new(audio_data),
            src_channels: channels,
            sample_rate,
        }
    }

    /// 再生を開始する
    pub fn play(&mut self, params: Arc<Mutex<DspParams>>) -> Result<()> {
        if self.is_playing.load(Ordering::SeqCst) {
            return Ok(());
        }

        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .context("オーディオ出力デバイスが見つかりません")?;

        // デバイスのデフォルト出力設定を取得
        let default_config = device
            .default_output_config()
            .context("デフォルト出力設定の取得に失敗しました")?;

        let device_sample_rate = default_config.sample_rate().0;
        let device_channels = default_config.channels() as usize;

        let config = cpal::StreamConfig {
            channels: device_channels as u16,
            sample_rate: cpal::SampleRate(device_sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let audio_data = Arc::clone(&self.audio_data);
        let play_position_bits = Arc::clone(&self.play_position_bits);
        let phase_counter_bits = Arc::clone(&self.phase_counter_bits);
        let src_channels = self.src_channels;
        let src_sample_rate = self.sample_rate as f64;
        let total_frames = self.audio_data.len() / self.src_channels;

        // リサンプリング比: ソース1フレームあたり何デバイスフレーム出力するか
        // → デバイス側1フレームごとにソース側を (src_rate / device_rate) フレーム進める
        let rate_ratio = src_sample_rate / device_sample_rate as f64;

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let current_params = params.lock().unwrap().clone();
                    let mut src_pos =
                        f64::from_bits(play_position_bits.load(Ordering::SeqCst));
                    let mut phase_pos =
                        f64::from_bits(phase_counter_bits.load(Ordering::SeqCst));

                    for frame in data.chunks_mut(device_channels) {
                        // ソース位置のループ処理
                        while src_pos >= total_frames as f64 {
                            src_pos -= total_frames as f64;
                        }

                        // 線形補間でソースサンプルを取得
                        let idx0 = src_pos.floor() as usize;
                        let idx1 = if idx0 + 1 >= total_frames { 0 } else { idx0 + 1 };
                        let frac = (src_pos - idx0 as f64) as f32;

                        // DSP: ソースのサンプルレート基準で位相を計算
                        let phase = dsp::frame_phase(
                            current_params.frequency,
                            src_sample_rate as f32,
                            phase_pos.floor() as u64,
                        );
                        let mod_value =
                            dsp::generate_mod_value(current_params.waveform, phase);

                        // デバイスの各チャンネルに出力
                        for (dev_ch, sample_out) in frame.iter_mut().enumerate() {
                            // ソースチャンネルへのマッピング
                            let src_ch = if src_channels == 1 {
                                0 // モノラル → 全チャンネルに同じ値
                            } else {
                                dev_ch % src_channels
                            };

                            let s0 = audio_data[idx0 * src_channels + src_ch];
                            let s1 = audio_data[idx1 * src_channels + src_ch];
                            let interpolated = s0 + (s1 - s0) * frac;

                            *sample_out = dsp::apply_ring_mod(
                                interpolated,
                                mod_value,
                                current_params.mix,
                            )
                            .clamp(-1.0, 1.0);
                        }

                        src_pos += rate_ratio;
                        phase_pos += rate_ratio;
                    }

                    play_position_bits.store(src_pos.to_bits(), Ordering::SeqCst);
                    phase_counter_bits.store(phase_pos.to_bits(), Ordering::SeqCst);
                },
                move |err| {
                    eprintln!("オーディオストリームエラー: {}", err);
                },
                None,
            )
            .context("オーディオストリームの構築に失敗しました")?;

        stream.play().context("オーディオ再生の開始に失敗しました")?;

        self.stream = Some(stream);
        self.is_playing.store(true, Ordering::SeqCst);

        Ok(())
    }

    /// 再生を停止する
    pub fn stop(&mut self) {
        self.stream = None;
        self.is_playing.store(false, Ordering::SeqCst);
    }

    /// 再生位置と位相をリセットする
    pub fn reset(&mut self) {
        self.play_position_bits
            .store(0.0_f64.to_bits(), Ordering::SeqCst);
        self.phase_counter_bits
            .store(0.0_f64.to_bits(), Ordering::SeqCst);
    }
}
