use crate::config::ini_parse;
use anyhow::{Context, Result, anyhow};
use std::{
    any, fs,
    io::ErrorKind,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Condvar, Mutex, OnceLock, RwLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, SystemTime},
};

use nix::{
    libc::free,
    sys::{
        statfs,
        statvfs::{self, statvfs},
    },
};

const VIDEO_DEVICE_MAX_COUNT: usize = 4;
const CHECK_INTERVAL_NORMAL: u64 = 60;
const CHECK_INTERVAL_ERROR: u64 = 5;
const LOW_SPACE_THRESHOLD_KB: u64 = 1024 * 512;
static EMMC: OnceLock<RwLock<Emmc>> = OnceLock::new();
static EMMC_CTRL: OnceLock<EmmcCheckCtrl> = OnceLock::new();
static EMMC_THREAD_QUIT: AtomicBool = AtomicBool::new(true);

#[derive(Debug, Clone)]
pub struct EmmcStatus {
    mount_status: bool,
    is_read_only: bool,
    total_size: u64,
    free_size: u64,
    used_size: u64,
}
#[derive(Debug, Clone)]
struct EmmcAttributes {
    emmc_devname: String,
    emmc_mntpoint: String,
    emmc_eventsdir: String,
    emmc_recorddir: String,
    video_device_dir: Vec<String>,
    tmp_events_dir: String,
}
struct EmmcCheckCtrl {
    force_check_cond: Condvar,
    force_check_lock: Mutex<bool>,
}
struct Emmc {
    inner: EmmcStatus,
    attributes: EmmcAttributes,
    remount_fail_count: u32,
    remove_status: bool,
}

enum EmmcStateType {
    CheckMount,
    CheckDirs,
    UpdateInfo,
    MountRetry,
}
struct CleanConfig {
    patterns: &'static [&'static str],
}
static CLEAN_MODES: [CleanConfig; 3] = [
    CleanConfig {
        patterns: &["*.h265", "*.h264", "*/*.h265", "*/*.h264"],
    },
    CleanConfig {
        patterns: &["*.jpeg", "*.jpg", "*/*.jpeg", "*/*.jpg"],
    },
    CleanConfig {
        patterns: &["*.h265", "*.h264", "*/*.h265", "*/*.h264"],
    },
];

//返回0表示成功
pub fn emmc_init() -> Option<i32> {
    let devname = ini_parse::ini_get_ini_config("system", "emmcdevname")?;
    let mntpoint = ini_parse::ini_get_ini_config("system", "emmcdevmnt")?;
    let eventsdir = ini_parse::ini_get_ini_config("system", "emmceventsdir")?;
    let recorddir = ini_parse::ini_get_ini_config("system", "emmcrecorddir")?;

    let emmc: Emmc = Emmc {
        inner: EmmcStatus {
            mount_status: false,
            is_read_only: true,
            total_size: 0,
            free_size: 0,
            used_size: 0,
        },
        attributes: EmmcAttributes {
            emmc_devname: devname,
            emmc_mntpoint: mntpoint,
            emmc_eventsdir: eventsdir,
            emmc_recorddir: recorddir,
            video_device_dir: (0..VIDEO_DEVICE_MAX_COUNT)
                .map(|num| format!("video_device{}", num))
                .collect::<Vec<String>>(),
            tmp_events_dir: String::from("/tmp/events"),
        },
        remount_fail_count: 0,
        remove_status: false,
    };
    let ctrl_lock = EmmcCheckCtrl {
        force_check_cond: Condvar::new(),
        force_check_lock: Mutex::new(false),
    };
    EMMC_CTRL.get_or_init(|| {
        println!("emmc ctrl_lock init ok.");
        ctrl_lock
    });
    EMMC.get_or_init(|| {
        println!("emmc int ok.");
        RwLock::new(emmc)
    });
    println!("{:#?}", EMMC.get()?.read().unwrap().attributes);
    Some(0)
}

//私有化了，暂时不用(no_use)
fn _none_emmc_set_config(emmc_attributes: EmmcAttributes) -> Option<i32> {
    EMMC.get()?.write().ok()?.attributes = emmc_attributes;
    Some(0)
}
//私有化了，暂时不用(no_use)
fn _none_emmc_get_config() -> Option<EmmcAttributes> {
    Some(EMMC.get()?.read().ok()?.attributes.clone())
}

pub fn emmc_get_events_path() -> Option<String> {
    let emmc = EMMC.get()?.read().ok()?;
    if emmc.inner.mount_status == false {
        Some(emmc.attributes.tmp_events_dir.clone())
    } else {
        Some(format!(
            "{}/{}",
            &emmc.attributes.emmc_mntpoint, &emmc.attributes.emmc_eventsdir
        ))
    }
}
pub fn emmc_get_recoder_path(chn: usize) -> Option<String> {
    if chn >= VIDEO_DEVICE_MAX_COUNT {
        return None;
    }
    let emmc = EMMC.get()?.read().ok()?;

    if emmc.inner.mount_status == false {
        return None;
    } else {
        Some(format!(
            "{}/{}/{}",
            &emmc.attributes.emmc_mntpoint,
            &emmc.attributes.emmc_recorddir,
            &emmc.attributes.video_device_dir.get(chn)?
        ))
    }
}

pub fn emmc_get_recoder_base_path() -> Option<String> {
    let emmc = EMMC.get()?.read().ok()?;
    if emmc.inner.mount_status == false {
        return None;
    } else {
        Some(format!(
            "{}/{}",
            &emmc.attributes.emmc_mntpoint, &emmc.attributes.emmc_recorddir
        ))
    }
}
// pub(crate) fn emmc_update_status(state: EmmcStatus) -> Option<i32> {
//     let mut emmc = EMMC.get()?.write().ok()?;
//     emmc.inner = state;
//     Some(0)
// }

pub fn emmc_get_mount_status() -> Option<bool> {
    Some(EMMC.get()?.read().ok()?.inner.mount_status)
}

pub fn emmc_get_info() -> Option<EmmcStatus> {
    let emmc = EMMC.get()?.read().ok()?;
    println!("emmc_get_info: {:#?}", emmc.inner);
    Some(emmc.inner.clone())
}

pub(crate) fn emmc_trigger_immediate_check() -> Option<i32> {
    let emmc = EMMC_CTRL.get()?;
    {
        *emmc.force_check_lock.lock().unwrap() = true;
    }
    emmc.force_check_cond.notify_all();
    Some(0)
}

pub(crate) fn emmc_interruptible_sleep(seconds: u64) -> Option<i32> {
    let emmc = EMMC_CTRL.get()?;

    if EMMC_THREAD_QUIT.load(Ordering::SeqCst) == true {
        return Some(0);
    }
    let lock = emmc.force_check_lock.lock().ok()?;

    let result = emmc
        .force_check_cond
        .wait_timeout_while(lock, Duration::from_secs(seconds), |force| {
            let temp = *force;
            *force = false;
            !temp
        })
        .ok()?;
    if result.1.timed_out() {
        Some(0)
    } else {
        Some(1)
    }
}

///Recursively delete eyery the oldest file in paths and subpaths
pub(crate) fn emmc_delete_oldest_file(path: &Path) -> Result<(), std::io::Error> {
    let mut old_file = SystemTime::now();
    let mut oldest_path: Option<PathBuf> = None;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let new_path = path.join(entry.file_name());
            // println!("{:?}", new_path);
            if let Err(err) = emmc_delete_oldest_file(&new_path) {
                eprintln!("recursion delete old file err:{:?}", err);
            };
        }
        if entry.file_type()?.is_file() {
            let file_info = entry.metadata()?;
            let cur_file_time = file_info.modified()?;
            if cur_file_time < old_file {
                old_file = cur_file_time;
                oldest_path = Some(path.join(entry.file_name()));
            }
        }
    }
    if let Some(path) = oldest_path {
        println! {"delete oldest file:{:?}", path};
        fs::remove_file(path)?;
    }
    Ok(())
}

fn emmc_update_info_locked(emmc: &mut Emmc) -> Option<bool> {
    let mount_point = emmc.attributes.emmc_mntpoint.clone();
    let stat = match statvfs(Path::new(&mount_point)) {
        Ok(ss) => ss,
        Err(err) => {
            println!("file path:{:?}  ,err:{:?}", mount_point, err);
            return None;
        }
    };
    let mount_status = true;
    let is_read_only = stat.flags().contains(statvfs::FsFlags::ST_RDONLY);
    let total_size = (stat.blocks() as u64 * stat.block_size() as u64) / 1024;
    let free_size = (stat.blocks_free() as u64 * stat.block_size() as u64) / 1024;
    let used_size = total_size - free_size;
    if total_size <= LOW_SPACE_THRESHOLD_KB {
        println!(
            "Low space detected (< {}KB), deleting oldest files...",
            LOW_SPACE_THRESHOLD_KB
        );
        let _ = emmc_delete_oldest_file(Path::new(&mount_point));
    }

    emmc.inner.free_size = free_size;
    emmc.inner.is_read_only = is_read_only;
    emmc.inner.mount_status = mount_status;
    emmc.inner.total_size = total_size;
    emmc.inner.used_size = used_size;
    Some(is_read_only)
}

pub(crate) fn emmc_update_info() -> Option<bool> {
    let mut emmc = EMMC.get()?.write().ok()?;
    emmc_update_info_locked(&mut emmc)
}

pub(crate) fn emmc_mounted_status() -> Option<bool> {
    let (device, mount_point) = {
        let emmc = match EMMC.get()?.read() {
            Ok(e) => e,
            Err(_) => {
                eprintln!("EMMC lock poisoned");
                return None;
            }
        };
        (
            emmc.attributes.emmc_devname.clone(),
            emmc.attributes.emmc_mntpoint.clone(),
        )
    };
    match fs::read_to_string("/proc/mounts") {
        Ok(content) => Some(content.lines().any(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            parts.len() >= 2 && parts[0] == device && parts[1] == mount_point
        })),
        Err(e) => {
            eprintln!("Failed to read /proc/mounts: {}", e);
            None
        }
    }
}

pub(crate) fn safe_mkdir(path: &Path) -> Result<(), std::io::Error> {
    if !path.exists() {
        fs::create_dir_all(path)?;
    }
    Ok(())
}
pub(crate) fn emmc_create_tmp_events_dirs() -> Option<i32> {
    let event_dir = {
        let emmc = EMMC.get()?.read().ok()?;
        emmc.attributes.tmp_events_dir.clone()
    };
    match safe_mkdir(Path::new(&event_dir)) {
        Ok(_) => {
            println!("Create emmc tmp dir success")
        }
        Err(err) => {
            eprintln!("Failed to create emmc tmp dir: {}", err);
        }
    };

    Some(0)
}

pub(crate) fn emmc_create_mount_dirs() -> Result<()> {
    let emmc = EMMC
        .get()
        .context("EMMC not initialized")?
        .read()
        .map_err(|e| anyhow!("Failed to acquire EMMC lock: {:?}", e))?;
    let dirs = [
        format!(
            "{}/{}",
            &emmc.attributes.emmc_mntpoint, &emmc.attributes.emmc_eventsdir
        ),
        format!(
            "{}/{}",
            &emmc.attributes.emmc_mntpoint, &emmc.attributes.emmc_recorddir
        ),
        format!("{}/{}", &emmc.attributes.emmc_mntpoint, "log"),
    ];
    for dir in &dirs {
        safe_mkdir(Path::new(&dir)).with_context(|| format!("Create dir {} failed", dir))?;
    }
    for chn in 0..VIDEO_DEVICE_MAX_COUNT {
        let record_dir = format!(
            "{}/{}/{}",
            &emmc.attributes.emmc_mntpoint,
            &emmc.attributes.emmc_recorddir,
            &emmc
                .attributes
                .video_device_dir
                .get(chn)
                .with_context(|| format!("video_device_dir {} not found", chn))?
        );
        safe_mkdir(Path::new(&record_dir))
            .with_context(|| format!("Create dir {} failed", record_dir))?;
    }
    Ok(())
}

pub(crate) fn emmc_try_mount() -> Result<()> {
    let mut emmc = EMMC
        .get()
        .context("EMMC not initialized")?
        .write()
        .map_err(|e| anyhow!("Failed to acquire EMMC lock: {:?}", e))?;
    let _ = Command::new("blockdev")
        .args(["--setrw", &emmc.attributes.emmc_devname])
        .status();
    let _ = Command::new("umount")
        .args(["-f", &emmc.attributes.emmc_mntpoint])
        .status();
    thread::sleep(Duration::from_millis(500));
    if emmc.remount_fail_count >= 3 {
        let mut output = Command::new("mkfs.ext4")
            .args(["-F", &emmc.attributes.emmc_devname])
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to spawn mkfs.ext4")?;

        if let Some(stderr) = output.stderr.take() {
            let reader = BufReader::new(stderr);
            for line in reader.lines().flatten() {
                println!("mkfs: {}", line);
            }
        }
        let status = output.wait()?;
        if !status.success() {
            emmc.remount_fail_count += 1;
            return Err(anyhow!("mkfs.ext4 failed"));
        }
        emmc.remount_fail_count = 0;
    } else {
        let mut output = Command::new("fsck.ext4")
            .args(["-y", &emmc.attributes.emmc_devname])
            .stdout(Stdio::piped())
            .spawn()
            .context("Failed to spawn fsck.ext4")?;
        if let Some(stdout) = output.stdout.take() {
            let reader = BufReader::new(stdout);
            for line in reader.lines().flatten() {
                println!("fsck: {}", line);
            }
        }
        let status = output.wait()?;
        match status.code() {
            Some(0) => println!("Filesystem ckeck completed"),
            Some(1) => println!("Errors corrected"),
            Some(2) => println!("System should be rebooted"),
            Some(c) if c >= 4 => {
                emmc.remount_fail_count += 1;
                eprintln!("Serious errors: {}", c);
                return Err(anyhow!("fsck failed with code {}", c));
            }
            _ => {
                eprintln!("Killed by signal");
                return Err(anyhow!("fsck killed"));
            }
        }
    }
    let status = Command::new("mount")
        .args([
            "-o",
            "noatime,nodiratime",
            &emmc.attributes.emmc_devname,
            &emmc.attributes.emmc_mntpoint,
        ])
        .status()
        .context("Failed to execute mount")?;
    if !status.success() {
        emmc.remount_fail_count += 1;
        return Err(anyhow!("Mount failed"));
    }

    thread::sleep(Duration::from_millis(500));
    match emmc_update_info_locked(&mut emmc) {
        Some(false) => {
            // false = 可写
            println!("Remount successful");
            emmc.remount_fail_count = 0;
            Ok(())
        }
        Some(true) => {
            // true = 只读
            eprintln!("Remount successful but still read-only");
            emmc.remount_fail_count += 1;
            Err(anyhow!("Still read-only after remount"))
        }
        None => {
            eprintln!("Failed to update EMMC info");
            emmc.remount_fail_count += 1;
            Err(anyhow!("Failed to verify mount"))
        }
    }
}

pub(crate) fn emmc_check_thread() {
    println!("emmc check thread start");
    let mut check_status = EmmcStateType::MountRetry;
    while EMMC_THREAD_QUIT.load(Ordering::Relaxed) == false {
        match check_status {
            EmmcStateType::CheckMount => {
                if let Some(true) = emmc_mounted_status() {
                    check_status = EmmcStateType::CheckDirs;
                } else {
                    check_status = EmmcStateType::MountRetry;
                };
            }
            EmmcStateType::CheckDirs => match emmc_create_mount_dirs() {
                Ok(_) => check_status = EmmcStateType::UpdateInfo,
                Err(e) => eprintln!("Failed to create dirs: {}", e),
            },
            EmmcStateType::UpdateInfo => {
                match emmc_update_info() {
                    Some(false) => {
                        eprintln!("emmc mountid normal");
                        println!("emmc remainfile: {:?}", emmc_get_remainfile_count());
                        emmc_interruptible_sleep(CHECK_INTERVAL_NORMAL);
                    }
                    _ => {
                        check_status = EmmcStateType::MountRetry;
                        eprintln!("emmc device is read only");
                        emmc_interruptible_sleep(CHECK_INTERVAL_ERROR);
                    }
                };
            }
            EmmcStateType::MountRetry => match emmc_try_mount() {
                Ok(_) => check_status = EmmcStateType::CheckMount,
                Err(e) => {
                    eprintln!("Failed to mount: {}", e);
                    emmc_interruptible_sleep(CHECK_INTERVAL_ERROR);
                }
            },
        }
    }
}

pub(crate) fn emmc_get_remainfile_count_inner(main_path: &Path) -> Result<u64> {
    let mut count = 0;

    for entry in fs::read_dir(main_path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_file()
            && entry
                .path()
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|ext| matches!(ext, "h264" | "h265" | "jpg" | "jpeg"))
        {
            count += 1;
        } else if file_type.is_dir() && !entry.metadata()?.file_type().is_symlink() {
            let new_path = main_path.join(entry.file_name());
            count += emmc_get_remainfile_count_inner(&new_path)?;
        }
    }
    Ok(count)
}

pub fn emmc_get_remainfile_count() -> Result<u64> {
    let event_path = emmc_get_events_path().context("get events path failed")?;
    let event_path = Path::new(&event_path);
    emmc_get_remainfile_count_inner(event_path)
}

pub(crate) fn emmc_safe_execute_rm(base_path: &str, pattern: &str) -> Result<()> {
    let full_path = format!("{}/{}", base_path, pattern);
    if full_path.len() > 400 {
        return Err(anyhow!("Path too long: {}", full_path));
    }
    println!("##### fullpath: {}", full_path);
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("rm -rf {}", full_path))
        .status()
        .context("Failed to execute rm command")?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("rm command failed with status: {}", status))
    }
}
pub fn emmc_clear(mode: i32) -> Result<()> {
    if !(0..=3).contains(&mode) {
        return Err(anyhow!("mode out of range"));
    }
    if emmc_get_mount_status() != Some(true) {
        return Err(anyhow!("emmc not mounted"));
    }
    let mut emmc = EMMC
        .get()
        .context("get emmc failed")?
        .write()
        .map_err(|e| anyhow!("Failed to acquire EMMC lock: {:?}", e))?;
    emmc.remove_status = true;
    if mode == 3 {
        let mut has_error = false;
        for i in 0..3 {
            if let Err(_) = emmc_clear(i) {
                has_error = true;
            }
        }
        emmc.remove_status = false;
        return if has_error {
            Err(anyhow!("Some clear operations failed"))
        } else {
            Ok(())
        };
    }
    let target_path = if mode == 0 {
        emmc_get_recoder_base_path().context("get recoder path failed")?
    } else {
        emmc_get_events_path().context("get events path failed")?
    };
    let config = &CLEAN_MODES[mode as usize];

    let mut has_error = false;
    for pattern in config.patterns {
        if let Err(e) = emmc_safe_execute_rm(&target_path, pattern) {
            eprintln!("clear mode {} failed: {}", mode, e);
            has_error = true;
        }
    }
    Command::new("sync").output().context("sync failed")?;
    emmc.remove_status = false;
    if has_error {
        Err(anyhow!("Clear operation completed with errors"))
    } else {
        Ok(())
    }
}
pub fn emmc_get_remove_status() -> Result<bool> {
    Ok(EMMC
        .get()
        .context("get emmc failed")?
        .read()
        .map_err(|e| anyhow!("Failed to acquire EMMC lock: {:?}", e))?
        .remove_status)
}
pub fn emmc_check_start() -> thread::JoinHandle<()> {
    EMMC_THREAD_QUIT.store(false, Ordering::Relaxed);
    // emmc_update_info();
    emmc_create_tmp_events_dirs();
    let join_handle = thread::spawn(emmc_check_thread);
    return join_handle;
}

pub fn emmc_check_stop(handle: thread::JoinHandle<()>) {
    EMMC_THREAD_QUIT.store(true, Ordering::Relaxed);
    emmc_trigger_immediate_check();
    let _ = handle.join();
    println!("emmc check thread stopped");
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn it_works() {
        let path = Path::new("/home/linux/test/testdir/errorfilepath");
        if let Err(err) = emmc_delete_oldest_file(&path) {
            println!("{:?}", err);
        };
    }
}
