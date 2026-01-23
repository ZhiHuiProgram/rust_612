use crate::config::ini_parse;
use fs4;
use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::{
        Condvar, Mutex, OnceLock, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime},
};

use nix::libc::{c_char, statvfs};
use std::ffi::CString;
use std::mem::MaybeUninit;

const VIDEO_DEVICE_MAX_COUNT: usize = 4;
const MOUNT_RETRY_WAIT: u64 = 3;
static EMMC: OnceLock<RwLock<Emmc>> = OnceLock::new();
static EMMC_CTRL: OnceLock<EmmcCheckCtrl> = OnceLock::new();
static EMMC_THREAD_QUIT: AtomicUsize = AtomicUsize::new(0);

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
    emmc_devbase: String,
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
    check_thread_status: bool,
}

enum EmmcStateType {
    Init,
    CheckMount,
    CheckDirs,
    UpdateInfo,
    MountRetry,
    Error,
}
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
            emmc_devbase: String::new(),
            video_device_dir: vec!["".to_string(); VIDEO_DEVICE_MAX_COUNT],
            tmp_events_dir: String::from("/tmp/events"),
        },
        remount_fail_count: 0,
        check_thread_status: true,
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
    let cc = emmc_update_info();
    println!("{:?}", cc);
    let emmc = EMMC.get()?.read().ok()?;
    if emmc.inner.mount_status == false {
        Some(emmc.attributes.tmp_events_dir.clone())
    } else {
        Some(format!(
            "{}/{}",
            emmc.attributes.emmc_mntpoint, &emmc.attributes.emmc_eventsdir
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
pub(crate) fn emmc_update_status(state: EmmcStatus) -> Option<i32> {
    let mut emmc = EMMC.get()?.write().ok()?;
    emmc.inner = state;
    Some(0)
}

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
    *emmc.force_check_lock.lock().unwrap() = true;
    emmc.force_check_cond.notify_all();
    Some(0)
}

pub(crate) fn emmc_interruptible_sleep(seconds: u64) -> Option<i32> {
    let emmc = EMMC_CTRL.get()?;

    if EMMC_THREAD_QUIT.load(Ordering::SeqCst) != 0 {
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

pub(crate) fn emmc_update_info() -> Option<i32> {
    let emmc = EMMC.get()?.read().ok()?;
    let stat = match fs4::statvfs(&emmc.attributes.emmc_mntpoint) {
        Ok(ss) => ss,
        Err(err) => {
            println!(
                "file path:{:?}  ,err:{:?}",
                &emmc.attributes.emmc_mntpoint, err
            );
            return None;
        }
    };
    let available = stat.free_space();
    let total = stat.total_space();

    let stat =
        nix::sys::statvfs::statvfs(Path::new(&emmc.attributes.emmc_mntpoint)).map_err(|e| {
            // update_emmc_status(0, 1, 0, 0, 0);
            -1
        }).ok()?;

    let total = (stat.blocks() as u64 * stat.block_size() as u64) / 1024;
    let avail = (stat.blocks_free() as u64 * stat.block_size() as u64) / 1024;
    let used = total - avail;
    let readonly = if stat.flags().contains(nix::sys::statvfs::FsFlags::ST_RDONLY) {
        1
    } else {
        0
    };
    println!(
        "available:{:?}, tatal:{:?}, readonly:{:?}",
        available, total, readonly
    );
    println!("available:{:?}, tatal:{:?}", available, total);
    Some(0)
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
