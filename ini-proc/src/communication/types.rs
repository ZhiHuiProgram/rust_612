#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Top = 0,
    TopLeft = 1,
    TopCenter = 2,
    TopRight = 3, // 右上  3
    MiddleLeft,   // 左中  4
    Center,       // 中心  5
    MiddleRight,  // 右中  6
    BottomLeft,   // 左下  7
    BottomCenter, // 中下  8
    BottomRight,  // 右下  9
    EXTRA,
    CoordinateAll = 11,
}

#[repr(u16)] // 对应C的enum大小
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum McuComMsgType {
    Unknown = 0,
    HeartBeat = 1,
    HeartBeatRep = 2,
    Manage = 3,
    ManageResp = 4,
    IpcData = 5,
    IpcDataResp = 6,
    OtaData = 7,
    OtaDataResp = 8,
    StateChangeHeartBeat = 9,
    StateChangeHeartBeatRep = 10,
    GpsData = 11,
    GpsDataResp = 12,
    Cmd = 13,
    CmdResp = 14,
}

#[repr(C)]
pub struct McuComPackage {
    pub head: McuComPackageHead,
    pub data: ComPackage,
}

impl McuComPackage {
    pub fn as_cmd(&self) -> Option<&CmdPackage> {
        if self.head.msg_type == McuComMsgType::Cmd as u16 {
            unsafe {
                return Some(&self.data.cmd_package);
            }
        } else {
            return None;
        }
    }
    pub fn as_heartbeat(&self) -> Option<&HeartbeatPackage> {
        if self.head.msg_type == McuComMsgType::HeartBeat as u16 {
            unsafe {
                return Some(&self.data.heartbeat_package);
            }
        } else {
            return None;
        }
    }
    pub fn as_heartbeat_reply(&self) -> Option<&HeartbeadReplyPackage> {
        if self.head.msg_type == McuComMsgType::HeartBeatRep as u16 {
            unsafe {
                return Some(&self.data.heartbeat_reply_package);
            }
        } else {
            return None;
        }
    }
    pub fn bytes_to_struct<T>(data: &[u8]) -> T {
        unsafe { std::ptr::read(data.as_ptr() as *const T) }
    }
}
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct McuComPackageHead {
    pub pack_head_flg: u16, // 报文起始标志
    pub data_len: u16,      // 数据部分长度，只用来发送确认时长度可以为0
    pub crc: u8,            // mcu_id开始的后面全部数据进行校验
    pub mcu_id: u8,         // 发送方MCU_ID
    pub sn: u16,            // 报文流水号
    pub src_sn: u16,        // 源请求流水号
    pub msg_type: u16,      // 消息类型（管理报文、数据报文、心跳报文）
}
pub const CRC_OFFSET: usize = 5;
pub const HEAD_SIZE: usize = size_of::<McuComPackageHead>();
const _: () = assert!(HEAD_SIZE == 12);

#[repr(C)]
pub union ComPackage {
    pub cmd_package: CmdPackage,
    pub heartbeat_package: HeartbeatPackage,
    pub heartbeat_reply_package: HeartbeadReplyPackage,
}
const _: () = assert!(std::mem::size_of::<ComPackage>() == 260);

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CmdPackage {
    pub cmd_type: u16,
    pub cmd_data_len: u16,
    pub data: [u8; 256],
}
pub const CMD_HEADER_SIZE: u16 = 4;
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct HeartbeatPackage {
    pub mcu_io_state: u32,       // IO状态
    pub mcu_adc_value: u16,      // ADC采样值
    pub mcu_lock_state: u16,     // 施封/上锁状态 1 0x31
    pub mcu_gps_state: u8,       // GPS状态
    pub mcu_gprs_state: u8,      // GPRS状态
    pub mcu_gprs_signal: u8,     // GPRS信号强度
    pub mcu_ble_state: u8,       // BLE蓝牙状态
    pub tf_size_total: u32,      // tf卡的总容量
    pub tf_size_free: u32,       // tf卡的可用容量
    pub remain_file: u32,        // 未上传文件数量
    pub time_s: u32,             // 时间（秒级）
    pub time_zone: u8,           // 时区
    pub local_record_status: u8, // 保留位（用于结构体4字节对齐）
    pub gb28181_status: u8,      // gb28181 状态
    pub ai_status: u8,           // AI检测状态
    pub alarm_status: u8,        // 异物入侵状态
    pub system_status: u8,       // 空闲状态
    pub camera_status: u8,       // 摄像头状态
    pub tf_status: u8,           // 内存卡状态 0正常 1故障
}
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct HeartbeadReplyPackage {
    src_sn: u16,
    reserve: u16,
    heartbeat_package: HeartbeatPackage,
}
