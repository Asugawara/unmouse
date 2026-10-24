use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "unmouse", about = "キーボードだけでマウスを操作する")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// 常駐して、ホットキーで起動する（既定）
    Run {
        /// 受けたキーイベントを標準エラーに1行ずつ出す
        #[arg(long)]
        verbose: bool,
    },
    /// 権限・tap・Secure Input・設定・モニターを確かめる
    Doctor,
    /// モニター、文字数、各モニターのセルの大きさを表示する
    Layout,
    /// 今のカーソル位置を表示する
    Cursor,
    /// 指定した点のラベルを表示する
    Path { x: f64, y: f64 },
    /// tap なしで起動し、キーを順に流す
    Demo {
        /// 空白区切りのキー（例: "l l k ; f"）
        #[arg(long)]
        keys: String,
        #[arg(long, default_value_t = 400)]
        delay_ms: u64,
        /// クリックするキー（space d s a g）を許可する
        #[arg(long)]
        allow_click: bool,
    },
}

#[cfg(target_os = "macos")]
fn main() -> Result<()> {
    use unmouse::platform::macos::commands;
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Run { verbose: false }) {
        Command::Run { verbose } => commands::run(verbose),
        Command::Doctor => commands::doctor(),
        Command::Layout => commands::layout(),
        Command::Cursor => commands::cursor(),
        Command::Path { x, y } => commands::path(x, y),
        Command::Demo {
            keys,
            delay_ms,
            allow_click,
        } => commands::demo(&keys, delay_ms, allow_click),
    }
}

#[cfg(not(target_os = "macos"))]
fn main() -> Result<()> {
    let _ = Cli::parse();
    anyhow::bail!("macOS のみ対応しています")
}
