mod common;
mod config;
mod storage;
use config::ini_parse::ini_init_config;
use std::thread;
use std::time::Duration;
use storage::emmc::*;
const INI_FILENAME: &str = "mc6357.ini";

fn main() {
    let ret = ini_init_config(INI_FILENAME);
    println!("ini init ret: {:?}", ret);
    let ret = storage::emmc::emmc_init();
    println!("emmc init ret: {:?}", ret);
    println!("emmc get event: {:?}", emmc_get_events_path());
    println!("emmc get recoder: {:?}", emmc_get_recoder_path(1));
    println!("emmc get info: {:?}", storage::emmc::emmc_get_info());
    
    let emmc_handle = emmc_check_start();

    thread::sleep(Duration::from_secs(5));

    // emmc_check_stop(emmc_handle);
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}
