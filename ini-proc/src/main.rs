mod config;
mod storage;
mod common;
use config::ini_parse::ini_init_config;

const INI_FILENAME: &str = "mc6357.ini";

fn main() {
    let ret = ini_init_config(INI_FILENAME);
    println!("ini init ret: {:?}", ret);
    let ret = storage::emmc::emmc_init();
    println!("emmc init ret: {:?}", ret);
    println!("emmc get event: {:?}", storage::emmc::emmc_get_events_path());
    println!("emmc get recoder: {:?}", storage::emmc::emmc_get_recoder_path(1));
    println!("emmc get info: {:?}", storage::emmc::emmc_get_info());
}

