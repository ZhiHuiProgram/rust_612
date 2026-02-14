use super::types::*;

use ParseErrorType::*;
use ParseResult::*;
use std::sync::atomic::{AtomicU16, Ordering::Relaxed};
use tokio::sync::mpsc;
const CMD_TYPE: u16 = McuComMsgType::Cmd as u16;
const HEARTBEAT_TYPE: u16 = McuComMsgType::HeartBeat as u16;
const HEARTBEAT_RESP_TYPE: u16 = McuComMsgType::HeartBeatRep as u16;
const MUC_ID: u8 = 0x01;
const PACKAGE_HEAD_FLAG: u16 = 0xAA55;
static LOCAL_SN: AtomicU16 = AtomicU16::new(0);
#[derive(Debug)]
pub(crate) enum ParseErrorType {
    DataTooShort,
    HeadTypeConvError,
    MsgDataTooLong,
    CmdDataLenError,
    CrcVerifyError,
    NoneError,
}
pub enum ParseResult {
    NeedMore,
    Success(McuComPackage),
    Error(ParseErrorType),
}
pub(crate) fn crc8(data: &[u8]) -> u8 {
    let mut crc: u8 = 0;
    for &b in data {
        crc ^= b;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0x8C
            } else {
                crc >> 1
            };
        }
    }
    crc
}
pub(crate) async fn protocol_parse(data: &[u8], tx: &mpsc::Sender<Vec<u8>>) -> i32 {
    let pack = match parse_package_head(data).await {
        Success(pack) => pack,
        NeedMore => return 1,
        Error(err) => {
            println!("parse_package_head error: {:?}", err);
            return -1;
        }
    };
    println!("package msg type: {}", pack.head.msg_type);
    match pack.head.msg_type {
        HEARTBEAT_TYPE => process_heartbeat(pack).await,
        CMD_TYPE => process_cmd(pack, tx).await,
        _ => {
            println!("unknown msg_type: {}", pack.head.msg_type);
        }
    }
    0
}

pub(crate) async fn process_cmd(pack: McuComPackage, tx: &mpsc::Sender<Vec<u8>>) {
    quick_reply(tx).await;
    if let Some(cmd) = pack.as_cmd() {
        match CmdType::try_from(cmd.cmd_type) {
            Ok(CmdType::RemoteCapture) => (),
            Ok(CmdType::SetIp) => {
                quick_reply(tx).await;
            }
            Ok(CmdType::SetCoordinate) => {
                common_respond(CmdType::SetCoordinate,1,tx).await;
            }
            Err(err) => {
                println!("cmdtype trabs err: {}", err);
            }
            Ok(CmdType::Unknown) => {
                println!("cmd_type: {}", cmd.cmd_type);
            }
            _ => {
                println!(
                    "Exists but has not been realized cmd_type: {}",
                    cmd.cmd_type
                );
            }
        }
    }
}

pub(crate) async fn process_heartbeat(pack: McuComPackage) {}
pub(crate) async fn parse_package_head(data: &[u8]) -> ParseResult {
    if data.len() < HEAD_SIZE {
        return NeedMore;
    }
    let pack: McuComPackage = McuComPackage::bytes_to_struct(data);
    if data.len() < HEAD_SIZE + pack.head.data_len as usize {
        return NeedMore;
    }
    if pack.head.data_len as usize > std::mem::size_of::<ComPackage>() {
        println!("data_len too long");
        return Error(CmdDataLenError);
    }

    let crc_end = pack.head.data_len as usize + HEAD_SIZE;
    if crc8(&data[CRC_OFFSET..crc_end]) != pack.head.crc {
        println!("cmd crc8 verify error");
        return Error(CrcVerifyError);
    }

    match pack.head.msg_type {
        CMD_TYPE => {
            if let Some(cmd) = pack.as_cmd() {
                let expected = pack.head.data_len - CMD_HEADER_SIZE;
                if cmd.cmd_data_len != expected {
                    return Error(CmdDataLenError);
                }
            }
        }
        HEARTBEAT_TYPE | HEARTBEAT_RESP_TYPE => {}
        _ => return Error(HeadTypeConvError),
    }
    println!("parse_package_head: {:#?}", pack.head);
    Success(pack)
}
pub(crate) async fn protocol_package_send(
    head_data: ComPackage,
    msg_type: McuComMsgType,
    size: u16,
    tx: &mpsc::Sender<Vec<u8>>,
) {
    let pack_len = HEAD_SIZE + size as usize;
    LOCAL_SN.store(LOCAL_SN.load(Relaxed) + 1, Relaxed);
    let mut pack = McuComPackage {
        head: McuComPackageHead {
            pack_head_flg: PACKAGE_HEAD_FLAG.swap_bytes(),
            data_len: size,
            crc: 0,
            mcu_id: MUC_ID,
            sn: LOCAL_SN.load(Relaxed),
            src_sn: 0,
            msg_type: msg_type as u16,
        },
        data: head_data,
    };
    let buf = McuComPackage::struct_to_bytes(&pack);
    pack.head.crc = crc8(&buf[CRC_OFFSET..pack_len]);
    let _ = tx.try_send(McuComPackage::struct_to_bytes(&pack));
}
pub(crate) async fn quick_reply(tx: &mpsc::Sender<Vec<u8>>) {
    let mut cmd_pack = CmdPackage {
        cmd_type: 0x6666,
        cmd_data_len: 0,
        data: [0; 256],
    };
    let data = b"0";
    let len = data.len().min(cmd_pack.data.len());
    cmd_pack.data[..len].copy_from_slice(&data[..len]);
    cmd_pack.cmd_data_len = len as u16;

    let com_pack = ComPackage {
        cmd_package: cmd_pack, // 初始化 union 的一个字段
    };
    protocol_package_send(
        com_pack,
        McuComMsgType::CmdResp,
        cmd_pack.cmd_data_len + 4,
        tx,
    )
    .await;
    // let _ = tx.try_send(McuComPackage::struct_to_bytes(&cmd_pack));
}

pub(crate) async fn common_respond(cmdtype:CmdType,state: u16, tx: &mpsc::Sender<Vec<u8>>){
    let mut cmd_pack = CmdPackage {
        cmd_type: cmdtype as u16,
        cmd_data_len: 0,
        data: [0; 256],
    };
    let data = format!("{},{:04?}",state,333);
    let data = data.as_bytes();
    let len = data.len().min(cmd_pack.data.len());
    cmd_pack.data[..len].copy_from_slice(&data[..len]);
    cmd_pack.cmd_data_len = len as u16;

    let com_pack = ComPackage {
        cmd_package: cmd_pack, // 初始化 union 的一个字段
    };
    protocol_package_send(
        com_pack,
        McuComMsgType::CmdResp,
        cmd_pack.cmd_data_len + 4,
        tx,
    )
    .await;
}