//! LaunchAgent への登録（install / uninstall / restart）。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result, bail};

const LABEL: &str = "io.github.asugawara.unmouse.daemon";
/// 識別子を変える前の版が登録していたラベル。install / uninstall で片付ける。
const LEGACY_LABEL: &str = "dev.unmouse.daemon";
const LAUNCHCTL: &str = "/bin/launchctl";
const CODESIGN: &str = "/usr/bin/codesign";

unsafe extern "C" {
    fn getuid() -> u32;
}

/// 手順の 1 つ。順序は `plan_*` が決め、`execute` が実行する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Bootout,
    WaitUnloaded,
    InstallBinary,
    WritePlist,
    Bootstrap,
    Kickstart,
    RemoveFiles,
}

pub fn plan_install(loaded: bool) -> Vec<Step> {
    let mut steps = Vec::new();
    // 実行中のバイナリは上書きできない（署名が壊れて落ちる）ので、必ず bootout が先。
    if loaded {
        steps.extend([Step::Bootout, Step::WaitUnloaded]);
    }
    steps.extend([Step::InstallBinary, Step::WritePlist, Step::Bootstrap]);
    steps
}

pub fn plan_uninstall(loaded: bool) -> Vec<Step> {
    let mut steps = Vec::new();
    if loaded {
        steps.extend([Step::Bootout, Step::WaitUnloaded]);
    }
    steps.push(Step::RemoveFiles);
    steps
}

pub fn plan_restart(plist_exists: bool, loaded: bool) -> Result<Vec<Step>> {
    if !plist_exists {
        bail!("登録されていません。先に `unmouse install` を実行してください");
    }
    Ok(if loaded { vec![Step::Kickstart] } else { vec![Step::Bootstrap] })
}

/// `launchd` に渡す plist（XML）。
pub fn render_plist(exe: &Path, log: &Path) -> String {
    let exe = xml_escape(&exe.to_string_lossy());
    let log = xml_escape(&log.to_string_lossy());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
        <string>run</string>
        <string>--wait-for-permission</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <dict>
        <key>Crashed</key>
        <true/>
    </dict>
    <key>ProcessType</key>
    <string>Interactive</string>
    <key>LimitLoadToSessionType</key>
    <string>Aqua</string>
    <key>StandardOutPath</key>
    <string>{log}</string>
    <key>StandardErrorPath</key>
    <string>{log}</string>
</dict>
</plist>
"#
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// 旧ラベルの plist の場所。
fn legacy_plist_path(home: &Path) -> PathBuf {
    home.join(format!("Library/LaunchAgents/{LEGACY_LABEL}.plist"))
}

pub fn home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME が設定されていません")
}

struct Paths {
    exe: PathBuf,
    plist: PathBuf,
    legacy_plist: PathBuf,
    log: PathBuf,
}

impl Paths {
    fn new() -> Result<Self> {
        let home = home()?;
        Ok(Self {
            exe: home.join("Library/Application Support/unmouse/unmouse"),
            plist: home.join(format!("Library/LaunchAgents/{LABEL}.plist")),
            legacy_plist: legacy_plist_path(&home),
            log: home.join("Library/Logs/unmouse.log"),
        })
    }
}

fn domain() -> String {
    format!("gui/{}", unsafe { getuid() })
}

fn target_of(label: &str) -> String {
    format!("{}/{label}", domain())
}

fn target() -> String {
    target_of(LABEL)
}

fn launchctl(args: &[&str]) -> Result<Output> {
    Command::new(LAUNCHCTL)
        .args(args)
        .output()
        .with_context(|| format!("launchctl {} を実行できません", args.join(" ")))
}

/// 読み込み済みか。終了コードだけで判定する。
fn is_loaded() -> bool {
    is_label_loaded(LABEL)
}

fn is_label_loaded(label: &str) -> bool {
    launchctl(&["print", &target_of(label)]).is_ok_and(|o| o.status.success())
}

fn launchctl_checked(args: &[&str]) -> Result<()> {
    let out = launchctl(args)?;
    if !out.status.success() {
        bail!(
            "launchctl {} が失敗しました（{}）: {}",
            args.join(" "),
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

fn wait_unloaded(label: &str) -> Result<()> {
    for _ in 0..50 {
        if !is_label_loaded(label) {
            return Ok(());
        }
        sleep(Duration::from_millis(200));
    }
    bail!("10 秒待っても {label} が終了しません")
}

fn bootstrap(plist: &Path) -> Result<()> {
    let plist_arg = plist.to_string_lossy();
    let out = launchctl(&["bootstrap", &domain(), &plist_arg])?;
    if out.status.success() {
        return Ok(());
    }
    let disabled = launchctl(&["print-disabled", &domain()])
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(&format!("\"{LABEL}\" => disabled")))
        .unwrap_or(false);
    if disabled {
        bail!("システム設定 > 一般 > ログイン項目で unmouse を許可してください");
    }
    bail!(
        "launchctl bootstrap が失敗しました（{}）: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr).trim()
    )
}

fn install_binary(paths: &Paths) -> Result<()> {
    let current = std::env::current_exe()
        .context("実行ファイルの場所を取得できません")?
        .canonicalize()
        .context("実行ファイルの場所を解決できません")?;
    if current.to_string_lossy().contains("/target/debug/") {
        eprintln!("警告: debug ビルドを常駐させようとしています（cargo build --release を使ってください）");
    }
    let dir = paths.exe.parent().context("コピー先の親ディレクトリがありません")?;
    fs::create_dir_all(dir).with_context(|| format!("{} を作れません", dir.display()))?;
    if let Some(parent) = paths.log.parent() {
        fs::create_dir_all(parent).with_context(|| format!("{} を作れません", parent.display()))?;
    }
    if paths.exe.canonicalize().is_ok_and(|dest| dest == current) {
        return Ok(());
    }
    let tmp = dir.join("unmouse.tmp");
    fs::copy(&current, &tmp).with_context(|| format!("{} へコピーできません", tmp.display()))?;
    let identity = std::env::var("UNMOUSE_SIGN_IDENTITY").unwrap_or_else(|_| "-".to_string());
    let out = Command::new(CODESIGN)
        .args(["--force", "--sign", &identity])
        .arg(&tmp)
        .output()
        .context("codesign を実行できません")?;
    if !out.status.success() {
        let _ = fs::remove_file(&tmp);
        bail!(
            "codesign が失敗しました（{}）: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    fs::rename(&tmp, &paths.exe).with_context(|| format!("{} へ置き換えられません", paths.exe.display()))
}

fn write_plist(paths: &Paths) -> Result<()> {
    if let Some(parent) = paths.plist.parent() {
        fs::create_dir_all(parent).with_context(|| format!("{} を作れません", parent.display()))?;
    }
    fs::write(&paths.plist, render_plist(&paths.exe, &paths.log))
        .with_context(|| format!("{} に書けません", paths.plist.display()))
}

fn remove_files(paths: &Paths) -> Result<()> {
    for path in [&paths.plist, &paths.exe] {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| format!("{} を削除できません", path.display())),
        }
    }
    Ok(())
}

/// 旧ラベルの登録を外し、plist を消す。登録されていなければ何もしない。
fn remove_legacy(paths: &Paths) -> Result<()> {
    if is_label_loaded(LEGACY_LABEL) {
        // 「見つからない」などの失敗は無視する。外れたかどうかは wait_unloaded が確かめる。
        let _ = launchctl(&["bootout", &target_of(LEGACY_LABEL)]);
        wait_unloaded(LEGACY_LABEL)?;
    }
    match fs::remove_file(&paths.legacy_plist) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("{} を削除できません", paths.legacy_plist.display())),
    }
}

fn execute(steps: &[Step], paths: &Paths) -> Result<()> {
    for step in steps {
        match step {
            Step::Bootout => {
                // 終了待ちの途中で失敗を返すことがある。外れたかどうかは次の WaitUnloaded が確かめる。
                if let Err(e) = launchctl_checked(&["bootout", &target()]) {
                    eprintln!("警告: {e:#}");
                }
            }
            Step::WaitUnloaded => wait_unloaded(LABEL)?,
            Step::InstallBinary => install_binary(paths)?,
            Step::WritePlist => write_plist(paths)?,
            Step::Bootstrap => bootstrap(&paths.plist)?,
            Step::Kickstart => launchctl_checked(&["kickstart", "-k", &target()])?,
            Step::RemoveFiles => remove_files(paths)?,
        }
    }
    Ok(())
}

pub fn install() -> Result<()> {
    let paths = Paths::new()?;
    // 旧ラベルの常駐も同じ実行ファイルを使うので、置き換える前に外す。
    remove_legacy(&paths)?;
    execute(&plan_install(is_loaded()), &paths)?;
    println!("インストールしました: {}", paths.exe.display());
    println!("plist: {}", paths.plist.display());
    println!("Accessibility の許可が必要です。初回は許可ダイアログが出ます（許可したあとに動き出さなければ unmouse restart）");
    Ok(())
}

pub fn uninstall() -> Result<()> {
    let paths = Paths::new()?;
    remove_legacy(&paths)?;
    execute(&plan_uninstall(is_loaded()), &paths)?;
    println!("削除しました（ログは残しています: {}）", paths.log.display());
    Ok(())
}

pub fn restart() -> Result<()> {
    let paths = Paths::new()?;
    let steps = plan_restart(paths.plist.exists(), is_loaded())?;
    execute(&steps, &paths)?;
    println!("起動しました");
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::process::Stdio;

    use super::*;

    fn plutil(args: &[&str], input: &str) -> (bool, String) {
        let mut child = Command::new("/usr/bin/plutil")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
        let out = child.wait_with_output().unwrap();
        (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn extract(plist: &str, keypath: &str) -> String {
        let (ok, out) = plutil(&["-extract", keypath, "raw", "-o", "-", "-"], plist);
        assert!(ok, "plutil -extract {keypath} が失敗しました");
        // raw は値の末尾に改行を 1 つ付ける。
        out.strip_suffix('\n').unwrap_or(&out).to_string()
    }

    #[test]
    fn plist_roundtrips_awkward_paths() {
        let exe = Path::new("/tmp/a & <b> c/unmouse");
        let log = Path::new("/tmp/x & y/unmouse.log");
        let plist = render_plist(exe, log);
        assert_eq!(extract(&plist, "ProgramArguments.0"), "/tmp/a & <b> c/unmouse");
        assert_eq!(extract(&plist, "ProgramArguments.1"), "run");
        assert_eq!(extract(&plist, "ProgramArguments.2"), "--wait-for-permission");
        assert_eq!(extract(&plist, "KeepAlive.Crashed"), "true");
        assert_eq!(extract(&plist, "LimitLoadToSessionType"), "Aqua");
        assert_eq!(extract(&plist, "StandardOutPath"), "/tmp/x & y/unmouse.log");
        assert_eq!(extract(&plist, "StandardErrorPath"), "/tmp/x & y/unmouse.log");
        assert!(!plist.contains('~'));
        assert!(plutil(&["-lint", "-"], &plist).0);
    }

    #[test]
    fn legacy_plist_lives_in_launch_agents() {
        assert_eq!(
            legacy_plist_path(Path::new("/Users/me")),
            Path::new("/Users/me/Library/LaunchAgents/dev.unmouse.daemon.plist")
        );
    }

    #[test]
    fn install_plans() {
        assert_eq!(
            plan_install(false),
            [Step::InstallBinary, Step::WritePlist, Step::Bootstrap]
        );
        let loaded = plan_install(true);
        assert_eq!(
            loaded,
            [
                Step::Bootout,
                Step::WaitUnloaded,
                Step::InstallBinary,
                Step::WritePlist,
                Step::Bootstrap
            ]
        );
        let pos = |s| loaded.iter().position(|x| *x == s).unwrap();
        assert!(pos(Step::Bootout) < pos(Step::InstallBinary));
    }

    #[test]
    fn uninstall_plans() {
        assert_eq!(plan_uninstall(false), [Step::RemoveFiles]);
        assert_eq!(
            plan_uninstall(true),
            [Step::Bootout, Step::WaitUnloaded, Step::RemoveFiles]
        );
    }

    #[test]
    fn restart_plans() {
        assert!(plan_restart(false, false).is_err());
        assert!(plan_restart(false, true).is_err());
        assert_eq!(plan_restart(true, false).unwrap(), [Step::Bootstrap]);
        assert_eq!(plan_restart(true, true).unwrap(), [Step::Kickstart]);
    }
}
