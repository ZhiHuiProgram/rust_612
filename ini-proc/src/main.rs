mod common;
mod communication;
mod config;
mod storage;
use config::ini_parse::ini_init_config;
use std::thread;
use std::time::Duration;
use storage::emmc::*;
const INI_FILENAME: &str = "mc6357.ini";

#[tokio::main]
async fn main() {
    common::setup_crash_handler();

    let ret = ini_init_config(INI_FILENAME);
    println!("ini init ret: {:?}", ret);
    let ret = storage::emmc::emmc_init();
    println!("emmc init ret: {:?}", ret);
    println!("emmc get event: {:?}", emmc_get_events_path());
    println!("emmc get recoder: {:?}", emmc_get_recoder_path(1));
    println!("emmc get info: {:?}", storage::emmc::emmc_get_info());

    let emmc_handle = emmc_check_start();

    // thread::sleep(Duration::from_secs(5));

    unsafe {
        println!("[TEST] Write to 0x1");
        std::ptr::write_volatile(0x1 as *mut u8, 42);
    }

    // emmc_check_stop(emmc_handle);
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}
