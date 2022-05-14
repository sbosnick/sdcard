// Copyright 2022 Steven Bosnick
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE-2.0 or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms

//! SD Card commands and app commands

/// Encode a GoIdleState command
// TODO: remove this when it is no longer needed
#[allow(dead_code)]
pub fn go_idle_state(buffer: &mut [u8]) {
    Cmd::GoIdleState.encode(0, buffer)
}

/// Encode a SendIfCond command assuming  2.7-3.6 V as the voltage supplied.
// TODO: remove this when it is no longer needed
#[allow(dead_code)]
pub fn send_if_cond(check_pattern: u8, buffer: &mut [u8]) {
    const VHS: u32 = 0b0001;
    Cmd::SendIfCond.encode((VHS << 8) | (check_pattern as u32), buffer)
}

/// Encode an AppCmd command. The next command should be an application command.
// TODO: remove this when it is no longer needed
#[allow(dead_code)]
pub fn app_cmd(buffer: &mut [u8]) {
    Cmd::AppCmd.encode(0, buffer);
}

/// Encode an SdSendOpCond app command.
// TODO: remove this when it is no longer needed
#[allow(dead_code)]
pub fn sd_send_op_cond(hcs: HostCapacitySupport, buffer: &mut [u8]) {
    AppCmd::SdSendOpCond.encode(hcs.to_arg(), buffer);
}

/// Host support for differend SD Card capacities.
// TODO: remove this when it is no longer needed
#[allow(dead_code)]
pub enum HostCapacitySupport {
    /// SDSC only host.
    ScOnly,

    /// SDHC or SDXC supported by host.
    HcOrXcSupported,
}

// This enum has all of the allowed commands for an SD Card in SPI mode,
// including ones that this package does not use. This is taken from Table 7-3
// of the Simplifed Specification.
#[allow(dead_code)]
#[repr(u8)]
#[derive(Clone, Copy)]
enum Cmd {
    GoIdleState = 0,
    SendOpCond = 1,
    SwitchFunc = 6,
    SendIfCond = 8,
    SendCSD = 9,
    SendCID = 10,
    StopTransmisson = 12,
    SendStatus = 13,
    SendBlockLen = 16,
    ReadSingleBlock = 17,
    ReadMultipleBlock = 18,
    WriteBlock = 24,
    WriteMultipleBlock = 25,
    ProgramCSD = 27,
    SetWriteProt = 28,
    ClrWriteProt = 29,
    SendWriteProt = 30,
    EraseWrBlkStartAddr = 32,
    EraseWrBlkEndAddr = 33,
    Erase = 38,
    LockUnlock = 42,
    AppCmd = 55,
    GenCmd = 56,
    ReadOCR = 58,
    CRCOnOff = 59,
}

// This enum has all of the allowed application specific commends for an SD Card
// in SPI mode including ones that this package does not use. This is taken from
// Table 7-4 of the Simplifed Specification.
#[allow(dead_code)]
#[repr(u8)]
#[derive(Clone, Copy)]
enum AppCmd {
    SdStatus = 13,
    SendNumWrBlocks = 22,
    SetWrBlkEraseCount = 23,
    SdSendOpCond = 41,
    SetClrCardDetect = 42,
    SendSCR = 51,
}

impl Encode for Cmd {
    fn start_byte(self) -> u8 {
        self as u8 | CMD_START
    }

    fn end_byte(self) -> u8 {
        // Section 7.2.2 of the Simplifed Specification allows for SPI mode to
        // work without a true CRC (bits 1-7 of the end byte) except for the
        // GoIdleState command. That secction gives 0x95 as a valid last byte
        // for the GoIdleState command. If we ever enable the SPI mode CRC
        // (with the CRCOnOff command) then we will have to rethink this
        // approach.
        match self {
            Cmd::GoIdleState => 0x95,
            _ => CMD_END,
        }
    }
}

impl Encode for AppCmd {
    fn start_byte(self) -> u8 {
        self as u8 | CMD_START
    }

    fn end_byte(self) -> u8 {
        // This assumes that we have not enabled SPI CRC mode.
        CMD_END
    }
}

trait Encode: Sized + Copy {
    fn start_byte(self) -> u8;
    fn end_byte(self) -> u8;

    fn encode(self, arg: u32, buffer: &mut [u8]) {
        assert!(buffer.len() >= 6, "Buffer to small to encode command.");

        buffer[0] = self.start_byte();
        buffer[1] = (arg >> 24) as u8;
        buffer[2] = (arg >> 16) as u8;
        buffer[3] = (arg >> 8) as u8;
        buffer[4] = (arg >> 0) as u8;
        buffer[5] = self.end_byte();
    }
}

impl HostCapacitySupport {
    fn to_arg(self) -> u32 {
        const HCR_BIT: u32 = 0b0100_0000_0000_0000_0000_0000_0000_0000;
        match self {
            HostCapacitySupport::ScOnly => 0,
            HostCapacitySupport::HcOrXcSupported => HCR_BIT,
        }
    }
}

// This is a start bit (0) followed by the transmittions from host bit (see
// Table 7-1 in the Simplifed Specification).
const CMD_START: u8 = 0b01000000;

// This is the end bit (1) for a command (see Table 7-1).
const CMD_END: u8 = 0b00000001;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmd_start_byte_includes_start_bits() {
        assert_eq!(Cmd::GoIdleState.start_byte(), 0x40);
        assert_eq!(Cmd::SendOpCond.start_byte(), 0x41);
    }

    #[test]
    fn go_idle_cmd_end_byte_includes_crc() {
        // This end byte is the one given in section 7.2.2 of the Simplifed
        // Specification.
        assert_eq!(Cmd::GoIdleState.end_byte(), 0x95);
    }

    #[test]
    fn other_cmd_end_byte_is_stop_bit() {
        assert_eq!(Cmd::SendOpCond.end_byte(), 0x01)
    }

    #[test]
    #[should_panic]
    fn cmd_encode_too_small_buffer_panics() {
        let mut buffer = [0; 5];
        Cmd::GoIdleState.encode(0, &mut buffer)
    }

    #[test]
    fn go_idle_cmd_encodes_as_specifified() {
        let mut buffer = [0; 6];

        Cmd::GoIdleState.encode(0, &mut buffer);

        // This is the encoding given in section 7.2.2 of the Simplifed
        // Specification.
        assert_eq!(buffer, [0x40, 0x00, 0x00, 0x00, 0x00, 0x95]);
    }

    #[test]
    fn read_single_block_cmd_encodes_as_expected() {
        let mut buffer = [0; 6];
        let addr = 0x12345678;

        Cmd::ReadSingleBlock.encode(addr, &mut buffer);

        assert_eq!(buffer, [0x51, 0x12, 0x34, 0x56, 0x78, 0x01]);
    }

    #[test]
    fn sd_status_cmd_encodes_as_expected() {
        let mut buffer = [0; 6];

        AppCmd::SdStatus.encode(0, &mut buffer);

        assert_eq!(buffer, [0x4d, 0x00, 0x00, 0x00, 0x00, 0x01]);
    }

    #[test]
    fn set_wr_blk_erase_count_cmd_encodes_as_expected() {
        let mut buffer = [0; 6];

        AppCmd::SetWrBlkEraseCount.encode(0x01, &mut buffer);

        assert_eq!(buffer, [0x57, 0x00, 0x00, 0x00, 0x01, 0x01]);
    }

    #[test]
    fn send_if_cond_encodes_as_expected() {
        let mut buffer = [0; 6];
        let check_pattern = 0x42;

        send_if_cond(check_pattern, &mut buffer);

        assert_eq!(buffer, [0x48, 0x00, 0x00, 0x01, check_pattern, 0x01]);
    }

    #[test]
    fn sd_send_op_code_encodes_as_expected() {
        let mut buffer = [0; 6];

        sd_send_op_cond(HostCapacitySupport::HcOrXcSupported, &mut buffer);

        assert_eq!(buffer, [0x69, 0x40, 0x00, 0x00, 0x00, 0x01])
    }
}
