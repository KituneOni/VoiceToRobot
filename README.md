# 🤖 Voice to Robot

WAVファイルにリングモジュレーションを適用し、ロボット風の音声に変換するツール。  
CLIによるバッチ処理と、GUIによるリアルタイムプレビューの両方に対応。

## 機能

- **リングモジュレーション** — サイン波・矩形波・ノコギリ波・三角波を選択可能
- **ドライ/ウェットミックス** — 原音と加工音の割合を 0.0〜1.0 で調整
- **CLIモード** — ストリーミング処理でGB単位のファイルも省メモリで変換
- **GUIモード** — パラメータを変更しながらリアルタイムプレビュー再生
- **対応フォーマット** — 16-bit PCM / 32-bit Float WAV

## 必要環境

- Rust (Edition 2021)

## ビルド

```sh
cargo build --release
```

## 使い方

### CLIモード

```sh
# 基本（デフォルト: サイン波 50Hz, ミックス 1.0）
voice-to-robot input.wav output.wav

# パラメータ指定
voice-to-robot input.wav output.wav -f 100 -w square -m 0.5

# 出力先が既存の場合は -y で上書き
voice-to-robot input.wav output.wav -y
```

#### CLI引数

| 引数 | 説明 | デフォルト |
|---|---|---|
| `input_path` | 入力WAVファイル（必須） | — |
| `output_path` | 出力WAVファイル（必須） | — |
| `-f, --frequency` | キャリア周波数 (Hz) | `50.0` |
| `-w, --waveform` | 波形 (`sine`, `square`, `saw`, `triangle`) | `sine` |
| `-m, --mix` | ミックス割合 (0.0=原音, 1.0=加工音100%) | `1.0` |
| `-y, --yes` | 出力先の上書きを許可 | `false` |

### GUIモード

引数なしで起動するとGUIが表示されます。

```sh
voice-to-robot
```

1. 「ファイルを開く」でWAVファイルを選択
2. 周波数スライダー・波形・ミックスを調整
3. 「再生」でリアルタイムプレビュー
4. 「エクスポート」で加工結果をWAV保存

## アルゴリズム

```
y[n] = x[n] × (1 - mix) + x[n] × mod_value × mix
```

- `mod_value`: 選択された波形で生成（周波数 `f`、サンプルレート `fs`、フレームインデックス `n`）
- ステレオ: 1フレーム（L+R）ごとに同一の `mod_value` を適用（位相ズレ防止）

## テスト

```sh
cargo test
```

30件のテスト（DSP単体テスト 10件 + CLI統合テスト 20件）。

## ライセンス

MIT
