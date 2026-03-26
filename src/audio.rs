use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleRate;

use crate::dsp::{self, DspParams};

/// オーディオプレイバックの状態を管理する
pub struct AudioPlayer {
    /// cpal ストリーム（Some の間は再生中）
    stream: Option<cpal::Stream>,
    /// 再生中フラグ
    pub is_playing: Arc<AtomicBool>,
    /// 現在の再生位置（フレーム単位）
    play_position: Arc<AtomicU64>,
    /// 累積位相インデックス（ループ時もリセットしない）
    phase_index: Arc<AtomicU64>,
    /// オーディオデータ（f32, インターリーブ）
    audio_data: Arc<Vec<f32>>,
    /// チャンネル数
    channels: usize,
    /// 元のサンプルレート
    pub sample_rate: u32,
}

impl AudioPlayer {
    /// 新しい AudioPlayer を作成する（まだ再生しない）
    pub fn new(audio_data: Vec<f32>, channels: usize, sample_rate: u32) -> Self {
        Self {
            stream: None,
            is_playing: Arc::new(AtomicBool::new(false)),
            play_position: Arc::new(AtomicU64::new(0)),
            phase_index: Arc::new(AtomicU64::new(0)),
            audio_data: Arc::new(audio_data),
            channels,
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

        let config = cpal::StreamConfig {
            channels: self.channels as u16,
            sample_rate: SampleRate(self.sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let audio_data = Arc::clone(&self.audio_data);
        let play_position = Arc::clone(&self.play_position);
        let phase_index = Arc::clone(&self.phase_index);
        let is_playing = Arc::clone(&self.is_playing);
        let channels = self.channels;
        let sample_rate = self.sample_rate as f32;
        let total_frames = self.audio_data.len() / self.channels;

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let current_params = params.lock().unwrap().clone();
                    let mut pos = play_position.load(Ordering::SeqCst) as usize;
                    let mut phase_idx = phase_index.load(Ordering::SeqCst);

                    for frame in data.chunks_mut(channels) {
                        if pos >= total_frames {
                            // ループ再生: 位置のみリセット、位相は継続
                            pos = 0;
                        }

                        let phase = dsp::frame_phase(current_params.frequency, sample_rate, phase_idx);
                        let mod_value = dsp::generate_mod_value(current_params.waveform, phase);

                        for (ch, sample_out) in frame.iter_mut().enumerate() {
                            let idx = pos * channels + ch;
                            let original = audio_data[idx];
                            *sample_out =
                                dsp::apply_ring_mod(original, mod_value, current_params.mix)
                                    .clamp(-1.0, 1.0);
                        }

                        pos += 1;
                        phase_idx += 1;
                    }

                    play_position.store(pos as u64, Ordering::SeqCst);
                    phase_index.store(phase_idx, Ordering::SeqCst);
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
        self.play_position.store(0, Ordering::SeqCst);
        self.phase_index.store(0, Ordering::SeqCst);
    }
}
