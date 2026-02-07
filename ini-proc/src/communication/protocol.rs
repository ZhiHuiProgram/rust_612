use super::types::*;

use ParseErrorType::*;
use ParseResult::*;

const CMD_TYPE: u16 = McuComMsgType::Cmd as u16;
const HEARTBEAT_TYPE: u16 = McuComMsgType::HeartBeat as u16;
const HEARTBEAT_RESP_TYPE: u16 = McuComMsgType::HeartBeatRep as u16;
const MUC_ID: u16 = 0x01;
const PackageHeadFlag: u16 = 0xAA55;

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
pub(crate) fn parse_package_head(data: &[u8]) -> ParseResult {
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
    println!("parse_package_head: {:?}", pack.head);
    Success(pack)
}
