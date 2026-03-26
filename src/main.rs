mod audio;
mod cli;
mod dsp;
mod gui;

use anyhow::Result;

fn main() -> Result<()> {
    // 引数がある場合はCLIモード、ない場合はGUIモード
    // clap のパースに干渉しないよう、最初の引数が help/version フラグか、
    // または位置引数（ファイルパス）であるかで判定する
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() {
        // 引数なし → GUIモード
        gui::run_gui()
    } else {
        // 引数あり → CLIモード
        cli::run_cli()
    }
}
