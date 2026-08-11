// Windows release 构建不弹控制台窗口。dev 构建保留，方便看日志。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(error) = vid2audio_desktop::run() {
        eprintln!("启动失败: {error}");
        std::process::exit(1);
    }
}
