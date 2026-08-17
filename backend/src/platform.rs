//! 跨平台差异集中在这里，其余模块不再散落 `cfg!(windows)`。
//!
//! 项目原本只在 Docker/Linux 下运行，很多约定（`/app/input`、`HOME`、PATH 里找
//! `ffmpeg`）在 Windows 桌面环境下会静默走偏而不是报错，所以统一收口。

use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// 展开路径开头的 `~`。
///
/// Windows 没有 `HOME`，要读 `USERPROFILE`；用户也习惯写 `~\Videos`，
/// 所以两种分隔符都接受。
pub fn expand_home(value: &str) -> PathBuf {
    let rest = value
        .strip_prefix("~/")
        .or_else(|| cfg!(windows).then(|| value.strip_prefix("~\\")).flatten());
    if let Some(rest) = rest
        && let Some(home) = home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(value)
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| {
            cfg!(windows)
                .then(|| std::env::var_os("USERPROFILE"))
                .flatten()
        })
        .map(PathBuf::from)
}

/// 可写的应用数据目录：数据库和日志放这里。
///
/// 容器里保持 `/app/data`；桌面版装到 Program Files 后当前目录不可写，
/// 必须落到用户目录，否则进程起不来。
///
/// 非 Windows 桌面版的相对路径 `data` 是坑：macOS 经 Finder/LaunchServices
/// 启动时工作目录是 `/`（根目录只读），SQLite 建库会直接 EROFS 崩溃；
/// 所以 macOS 落到 `~/Library/Application Support/vid2audio`，Linux 落到
/// XDG 数据目录。
pub fn default_data_dir() -> PathBuf {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|home| home.join("AppData").join("Local")))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("vid2audio")
    } else if Path::new("/app").is_dir() {
        PathBuf::from("/app/data")
    } else if cfg!(target_os = "macos") {
        home_dir()
            .map(|home| {
                home.join("Library")
                    .join("Application Support")
                    .join("vid2audio")
            })
            .unwrap_or_else(|| PathBuf::from("data"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|home| home.join(".local").join("share")))
            .unwrap_or_else(|| PathBuf::from("data"))
            .join("vid2audio")
    }
}

/// 默认扫描目录。容器里是挂载点，桌面上用「视频」文件夹。
pub fn default_input_dir() -> PathBuf {
    if cfg!(windows) {
        home_dir()
            .map(|home| home.join("Videos"))
            .unwrap_or_else(|| PathBuf::from("."))
    } else if Path::new("/app/input").is_dir() {
        PathBuf::from("/app/input")
    } else {
        home_dir()
            .map(|home| home.join("Videos"))
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

/// 默认输出目录。
///
/// Windows 上尤其不能沿用 `/app/output`：`PathBuf::from("/app/output")` 会被解析成
/// 「当前盘符根 + \app\output」，`create_dir_all` 于是静默在 `C:\app\output` 建目录，
/// 用户根本找不到自己的音频。
pub fn default_output_dir() -> PathBuf {
    if cfg!(windows) {
        home_dir()
            .map(|home| home.join("Music").join("Vid2Audio"))
            .unwrap_or_else(|| PathBuf::from("."))
    } else if Path::new("/app/output").is_dir() {
        PathBuf::from("/app/output")
    } else {
        home_dir()
            .map(|home| home.join("Music").join("Vid2Audio"))
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

/// 前端静态资源目录。桌面版随 exe 走，容器里在 `/app/static`。
pub fn default_static_dir(manifest_local: PathBuf) -> PathBuf {
    if manifest_local.is_dir() {
        return manifest_local;
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let beside = dir.join("static");
        if beside.is_dir() {
            return beside;
        }
    }
    PathBuf::from("/app/static")
}

/// 文件浏览器顶部的快捷入口。
///
/// Windows 上这不是锦上添花而是必需的：`Path::new("C:\\").parent()` 是 `None`，
/// 「上一级」走到盘根就断了，而故事机 / U 盘 / SD 卡几乎总挂在别的盘符上。
/// 没有盘符入口，用户只能手输 `D:\`——桌面程序不该这样。
pub fn filesystem_roots() -> Vec<(String, PathBuf)> {
    let mut roots = Vec::new();
    if let Some(home) = home_dir() {
        roots.push(("主目录".to_string(), home));
    }
    if cfg!(windows) {
        for letter in logical_drive_letters() {
            roots.push((format!("{letter}:"), PathBuf::from(format!("{letter}:\\"))));
        }
    } else {
        roots.push(("/".to_string(), PathBuf::from("/")));
    }
    roots
}

/// 当前存在的盘符。
///
/// 用 `GetLogicalDrives` 的位掩码而不是逐个 `is_dir("A:\\")` 试探：后者对
/// 断连的映射网络驱动器会阻塞数秒，一次浏览目录卡住整个界面。位掩码只是读一个
/// 内核里的值，不产生 I/O。
fn logical_drive_letters() -> Vec<char> {
    drive_letters_from_mask(logical_drive_mask())
}

#[cfg(windows)]
fn logical_drive_mask() -> u32 {
    // kernel32 在所有 windows target 上都是默认链接的，不需要额外的 crate。
    unsafe extern "system" {
        fn GetLogicalDrives() -> u32;
    }
    unsafe { GetLogicalDrives() }
}

#[cfg(not(windows))]
fn logical_drive_mask() -> u32 {
    0
}

/// `GetLogicalDrives` 的返回值：bit 0 是 A，bit 1 是 B，以此类推。
/// 抽出来是为了让位运算在非 Windows 上也能测。
fn drive_letters_from_mask(mask: u32) -> Vec<char> {
    (0..26u32)
        .filter(|index| mask & (1 << index) != 0)
        .map(|index| (b'A' + index as u8) as char)
        .collect()
}

/// 去掉 Windows `canonicalize` 加上的扩展长度前缀。
///
/// `std::fs::canonicalize` 在 Windows 上返回的是 `\\?\C:\Users\...`。这个前缀会一路
/// 漏到界面上（用户看不懂）和 ffmpeg 的命令行参数里（不是所有版本都认），而项目里
/// 每一个返回给前端的路径都经过 canonicalize。
///
/// 路径本身超长时保留前缀——那正是它存在的理由，去掉会让路径直接不可用。
pub fn strip_extended_prefix(value: &str) -> String {
    // MAX_PATH 是 260；留点余量，接近上限就不动它。
    if value.len() > 250 {
        return value.to_string();
    }
    // 网络路径是 `\\?\UNC\server\share`，直接切前缀会得到相对路径 `UNC\...`，
    // 要还原成 `\\server\share`。
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    value.strip_prefix(r"\\?\").unwrap_or(value).to_string()
}

/// Windows 上 PATH 里的可执行文件后缀，来自 `PATHEXT`。
fn executable_extensions() -> Vec<String> {
    if !cfg!(windows) {
        return Vec::new();
    }
    std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
        .split(';')
        .filter_map(|item| {
            let item = item.trim().trim_start_matches('.').to_lowercase();
            (!item.is_empty()).then_some(item)
        })
        .collect()
}

/// 桌面版随包分发的可执行文件目录。
///
/// 各平台放的位置不同——Windows 的 NSIS 装在 exe 同级，macOS 在
/// `Contents/Resources`，那里不是 exe 同级目录。与其在这里猜相对路径，
/// 不如让外壳启动时直接告知。
static BUNDLED_BIN_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// 由桌面版外壳在启动时调用，指向随包的 ffmpeg/ffprobe 所在目录。
///
/// 用 `OnceLock` 而不是改 `PATH`/环境变量：`std::env::set_var` 在 Rust 2024 是
/// unsafe，且此时 tokio 线程已经起来了，改环境变量存在数据竞争。
pub fn set_bundled_bin_dir(dir: PathBuf) {
    let _ = BUNDLED_BIN_DIR.set(dir);
}

fn bundled_bin_dir() -> Option<PathBuf> {
    if let Some(dir) = BUNDLED_BIN_DIR.get() {
        return Some(dir.clone());
    }
    // 容器/服务端部署仍可用环境变量指定。
    std::env::var_os("VID2AUDIO_BIN_DIR").map(PathBuf::from)
}

/// 在单个目录里找可执行文件：先试原名，再按 `PATHEXT` 逐个后缀试。
///
/// 只认 `.exe` 会漏掉 scoop 之类装出来的 `.cmd` 包装脚本。
fn find_in_dir(dir: &Path, name: &str, extensions: &[String]) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    extensions
        .iter()
        .map(|extension| dir.join(format!("{name}.{extension}")))
        .find(|candidate| candidate.is_file())
}

/// 找一个外部命令的完整路径。
///
/// 顺序：随包目录 → exe 同级目录 → `PATH`。用户自己装了 ffmpeg 也能用，
/// 随包的优先，免得撞上系统里某个残缺的旧版本。
pub fn find_command(name: &str) -> Option<PathBuf> {
    let extensions = executable_extensions();
    if let Some(dir) = bundled_bin_dir()
        && let Some(found) = find_in_dir(&dir, name, &extensions)
    {
        return Some(found);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
        && let Some(found) = find_in_dir(dir, name, &extensions)
    {
        return Some(found);
    }
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .find_map(|dir| find_in_dir(&dir, name, &extensions))
}

/// 构造一个不弹控制台窗口的命令。
///
/// GUI 程序里每次 spawn 都会闪一个黑窗口，提取几十集就闪几十次。
/// `CREATE_NO_WINDOW` = 0x0800_0000。
pub fn command(program: &Path) -> Command {
    // `mut` 只有 Windows 分支用得上，非 Windows 上标注 mut 会触发 unused_mut。
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}

/// Windows 上非法的文件名字符与保留设备名。
const WINDOWS_ILLEGAL: [char; 7] = ['<', '>', ':', '"', '|', '?', '*'];
const WINDOWS_RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// 检查一个文件名在 Windows 上是否合法，返回中文原因。
///
/// 这不只是「Windows 上会报错」的问题：`:` 尤其危险，因为 Windows 的
/// `PathBuf::push` 规定「带盘符前缀但无 root 时替换整个路径」，所以把文件重命名成
/// `C:evil` 会让 `parent.join(new_name)` 直接跳出父目录；而 `a.mp3:hidden`
/// 则是往 `a.mp3` 里写一条 NTFS 备用数据流，不是改名。
/// 因此**所有平台**都执行这套校验，不用 `cfg`。
pub fn reject_windows_unsafe_name(name: &str) -> Option<String> {
    if let Some(found) = name.chars().find(|c| WINDOWS_ILLEGAL.contains(c)) {
        return Some(format!("名称不能包含字符 {found}"));
    }
    if name.chars().any(|c| (c as u32) < 0x20) {
        return Some("名称不能包含控制字符".into());
    }
    if name.ends_with(' ') || name.ends_with('.') {
        return Some("名称不能以空格或点结尾".into());
    }
    let stem = name.split('.').next().unwrap_or(name).to_lowercase();
    if WINDOWS_RESERVED.contains(&stem.as_str()) {
        return Some(format!("{stem} 是系统保留名称，请换一个"));
    }
    None
}

/// 判断一个条目是否应视为隐藏。
///
/// Linux 看点前缀，Windows 看 `FILE_ATTRIBUTE_HIDDEN`/`SYSTEM`——不然
/// `desktop.ini`、`Thumbs.db` 会被当成普通文件排进播放顺序里。
pub fn is_hidden(path: &Path, name: &str) -> bool {
    if name.starts_with('.') {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const HIDDEN: u32 = 0x2;
        const SYSTEM: u32 = 0x4;
        if let Ok(metadata) = path.metadata() {
            return metadata.file_attributes() & (HIDDEN | SYSTEM) != 0;
        }
    }
    #[cfg(not(windows))]
    let _ = path;
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extended_length_prefixes_are_stripped_for_display() {
        assert_eq!(
            strip_extended_prefix(r"\\?\C:\Users\me\Videos"),
            r"C:\Users\me\Videos"
        );
        // UNC 前缀要还原成 `\\server\share`，不能切成相对路径 `UNC\...`。
        assert_eq!(
            strip_extended_prefix(r"\\?\UNC\nas\media\第一季"),
            r"\\nas\media\第一季"
        );
        // 没有前缀的路径（Linux、以及已经处理过的字符串）原样返回。
        assert_eq!(strip_extended_prefix("/videos/a.mp4"), "/videos/a.mp4");
        assert_eq!(strip_extended_prefix(r"D:\media"), r"D:\media");
        // 超长路径必须保留前缀，否则 Windows 直接打不开。
        let long = format!(r"\\?\C:\{}", "a".repeat(300));
        assert_eq!(strip_extended_prefix(&long), long);
    }

    #[test]
    fn drive_letters_come_from_the_bit_positions_of_the_mask() {
        // bit 0 = A、bit 2 = C：`GetLogicalDrives` 的约定。位序搞反的话
        // Windows 上会给出一串不存在的盘符，而这在 Linux 上根本跑不到。
        assert_eq!(drive_letters_from_mask(0b0000_0100), vec!['C']);
        assert_eq!(drive_letters_from_mask(0b0001_1100), vec!['C', 'D', 'E']);
        assert_eq!(drive_letters_from_mask(0), Vec::<char>::new());
        assert_eq!(drive_letters_from_mask(u32::MAX).len(), 26);
        assert_eq!(*drive_letters_from_mask(u32::MAX).last().unwrap(), 'Z');
    }

    #[test]
    fn rejects_names_that_escape_the_parent_directory_on_windows() {
        // 这条是安全用例，不是美观问题：`C:evil` 会让 join 跳出父目录。
        assert!(reject_windows_unsafe_name("C:evil").is_some());
        assert!(reject_windows_unsafe_name("a.mp3:hidden").is_some());
    }

    #[test]
    fn rejects_illegal_characters_reserved_names_and_trailing_dots() {
        for name in ["a<b", "a>b", "a\"b", "a|b", "a?b", "a*b"] {
            assert!(reject_windows_unsafe_name(name).is_some(), "{name}");
        }
        for name in ["CON", "con.mp3", "nul", "COM1", "lpt9.txt"] {
            assert!(reject_windows_unsafe_name(name).is_some(), "{name}");
        }
        assert!(reject_windows_unsafe_name("trailing ").is_some());
        assert!(reject_windows_unsafe_name("trailing.").is_some());
        assert!(reject_windows_unsafe_name("控制\u{1}符").is_some());
    }

    #[test]
    fn accepts_ordinary_names_including_chinese_and_zero_padded_output() {
        for name in [
            "001_植树节.mp3",
            "萌鸡小队第一季",
            "a.b.c.mkv",
            "第02集 找妈妈.mp4",
        ] {
            assert!(reject_windows_unsafe_name(name).is_none(), "{name}");
        }
    }

    #[test]
    fn expand_home_leaves_paths_without_a_tilde_alone() {
        assert_eq!(expand_home("/videos/a.mp4"), PathBuf::from("/videos/a.mp4"));
        assert_eq!(
            expand_home("relative/a.mp4"),
            PathBuf::from("relative/a.mp4")
        );
        // `~` 单独出现时不展开，避免把它当成目录名的一部分误伤。
        assert_eq!(expand_home("~abc"), PathBuf::from("~abc"));
    }

    #[test]
    fn expand_home_uses_the_home_directory_when_present() {
        let Some(home) = home_dir() else { return };
        assert_eq!(expand_home("~/videos"), home.join("videos"));
    }

    #[test]
    fn dotfiles_count_as_hidden_on_every_platform() {
        assert!(is_hidden(Path::new("/tmp/.ds_store"), ".ds_store"));
        assert!(!is_hidden(Path::new("/tmp/normal.mp4"), "normal.mp4"));
    }

    /// 随包目录要优先于 PATH：用户机器上可能有个残缺的旧 ffmpeg，
    /// 装了本程序就该用我们自己带的那个。这里测目录内的匹配规则——
    /// 带后缀的名字也要能被无后缀的查询命中，否则 Windows 上找不到 ffmpeg.exe。
    #[test]
    fn find_in_dir_matches_with_and_without_an_extension() {
        let root = std::env::temp_dir().join(format!("vid2audio-bin-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let extensions = ["exe".to_string(), "cmd".to_string()];

        // 只有带后缀的文件存在时，无后缀的查询要靠 PATHEXT 命中。
        std::fs::write(root.join("ffmpeg.exe"), b"x").unwrap();
        assert_eq!(
            find_in_dir(&root, "ffmpeg", &extensions),
            Some(root.join("ffmpeg.exe"))
        );

        // 无后缀的可执行文件（Linux/macOS 的常态）优先于带后缀的。
        std::fs::write(root.join("ffprobe"), b"x").unwrap();
        std::fs::write(root.join("ffprobe.cmd"), b"x").unwrap();
        assert_eq!(
            find_in_dir(&root, "ffprobe", &extensions),
            Some(root.join("ffprobe"))
        );

        // 不存在的命令不该返回任何东西。
        assert_eq!(find_in_dir(&root, "nonexistent", &extensions), None);

        std::fs::remove_dir_all(&root).unwrap();
    }
}
