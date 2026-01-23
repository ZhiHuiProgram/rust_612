use crate::common::FW_VERSION;
use ini::Ini;
use std::sync::{OnceLock, RwLock};

static CONFIG: OnceLock<RwLock<Ini>> = OnceLock::new();
pub fn ini_init_config(ini_filename: &str) -> Option<i32> {
    let ini = if let Ok(ini) = Ini::load_from_file(ini_filename) {
        if ini.len() == 1 {
            eprintln!("No sections found, writing default config.");
            ini_setting_default(ini_filename);
        }
        let ini = Ini::load_from_file(ini_filename).ok()?;
        for (sec, prop) in ini.iter() {
            println!("Section: {:?}", sec);
            // if prop.is_empty() {
            //     eprintln!("Section: {} is empty.",sec?);
            // }
            for (k, v) in prop.iter() {
                println!("{}:{}", k, v);
            }
        }
        ini
    } else {
        ini_setting_default(ini_filename)?
    };

    CONFIG.get_or_init(|| {
        println!("ini_init_config ok.");
        RwLock::new(ini)
    });
    Some(0)
}

pub fn ini_get_ini_config(section: &str, key: &str) -> Option<String> {
    CONFIG
        .get()?
        .read()
        .ok()?
        .get_from(Some(section), key)
        .map(|v| v.to_string())
}
fn ini_setting_default(ini_filename: &str) -> Option<Ini> {
    let mut conf = Ini::new();
    conf.with_section(None::<String>).set("soc", "mc6357");

    conf.with_section(Some("system"))
        .set("FW_VERSION", FW_VERSION)
        .set("LOG_LEVEL", "7")
        .set("lockstatus", "unlock")
        .set("serial", "/dev/ttyS1")
        .set("sddevname", "/dev/mmcblk1p1")
        .set("sddevmnt", "/media")
        .set("sdeventsdir", "events")
        .set("sdrecorddir", "record")
        .set("emmcdevname", "/dev/mmcblk0p9")
        .set("emmcdevmnt", "/data")
        .set("emmceventsdir", "events")
        .set("emmcrecorddir", "record")
        .set("recorder", "off")
        .set("yolov5s", "off")
        .set("rtmp_dev", "0");

    conf.with_section(Some("gpiopins"))
        .set("camctlbase", "37")
        .set("netpower", "33")
        .set("ownsidebusy", "41")
        .set("counterpartbusy", "5");

    conf.with_section(Some("quectel"))
        .set("ifname", "ppp0")
        .set("apn", "cmiot")
        .set("quectel_user", "test")
        .set("quectel_pwd", "test");

    conf.with_section(Some("network"))
        .set("interval", "10")
        .set("rtmp", "rtmp://vedio.hhdlink.online:1935/live/000000001332")
        .set("ftp_addr", "sdzt.hhdlink.online:21")
        .set("ftp_path", "/t31")
        .set("ftp_user", "test")
        .set("ftp_pwd", "hhd@123.com");

    conf.with_section(Some("gb28181"))
        .set("status", "off")
        .set("codeStream", "main")
        .set("serverIp", "219.134.62.202")
        .set("serverPort", "5060")
        .set("serverId", "44010200492504240018")
        .set("domain", "4401020049")
        .set("encode", "disable")
        .set("passWord", "admin123")
        .set("regTimeOut", "900")
        .set("heartBeat", "60")
        .set("deviceId", "44010200492504240018")
        .set("devicePort", "5060")
        .set("alertId", "0");

    if let Err(err) = conf.write_to_file(ini_filename) {
        eprintln!("Error: {}", err);
        None
    } else {
        Some(conf)
    }
}
