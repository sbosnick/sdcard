// Copyright 2022 Steven Bosnick
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE-2.0 or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms

//! Types to support SD Card SPI Mode responses.
//!
//! Every response starts with an [`R1Response`] even if the response format
//! for the command at issue is a different format. All of the other formats
//! start with the `R1` bit pattern in the first byte. Parsing this first byte
//! as its own type allows us to detect when the rest of the response has been
//! truncated (through [`R1Response::response_truncated`]) (see section 7.3.2
//! of the Simplified Specification). If [`R1Response::response_truncated`]
//! returns `true` then the remaining bytes of a non `R1` response will not
//! be sent from the card.
//!
//! The non-R1 responses currently implmented are:
//!     - R3
//!     - R7
//!
//! The non-R1 responses that are not yet implemented are:
//!     - R1b
//!     - R2

use core::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign};

use snafu::{ensure, Snafu};

use crate::common::{CardCapacity, VOLTAGE_2_7_TO_3_6};

/// Newtype to support decoding of an R1 response.
///
/// This type is based on section 7.3.2.1 of the Simplified Specification.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct R1Response(u8);

/// Newtype to support decoding of an R7 response.
///
/// This type decodes the last 4 bytes of the R7 response. The first byte
/// is an R1 response that should be decoded with [`R1Response`]. The remaining
/// bytes of the R7 response (after the R1 byte) will not be present if
/// [`R1Response::response_truncated`] is true.
///
/// This type is based on section 7.3.2.6 of the Simplified Specification.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct R7Response(u32, R1Response);

/// Newtype to support decoding the R3 response (and the OCR register).
///
/// This type decodes the last 4 bytes of the R3 response. The first byte
/// is an R1 response that should be decided with [`R1Response`]. The remaining
/// bytes of the R3 response (after the R1 byte) will not be present if
/// [`R1Response::response_truncated`] is true.
///
/// This type is based on section 7.3.2.4 of the Simplified Specification.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct R3Response(u32, R1Response);

/// Interface to create a response type from the initial R1 byte and the
/// remaining bytes for the response.
pub trait Response {
    /// The type of the extra bytes for the response.
    ///
    /// This should likely be [u8; N] for some N.
    type ExtraBytes: AsMut<[u8]> + Default;

    /// Create the response from the inital r1 byte and the SIZE -1 extra
    /// bytes.
    fn create(r1: R1Response, extra_bytes: &Self::ExtraBytes) -> Self;

    fn r1(&self) -> &R1Response;
}

#[derive(Debug, PartialEq, Snafu)]
pub enum ResponseError {
    #[snafu(display("SD Card detected an illegal command."))]
    IllegalCommand,

    #[snafu(display("SD Card detected a CRC check failure."))]
    ComCrcError,

    #[snafu(display("SD Card detected an erase seqeuence error."))]
    EraseSequenceError,

    #[snafu(display("SD Card detected an address error."))]
    AddressError,

    #[snafu(display("SD Card detected a paramater error."))]
    ParameterError,

    #[snafu(display("SD Card responded with an unexpected voltage."))]
    UnexpectVoltage,

    #[snafu(display("SD Card responded with unexpected check pattern."))]
    CheckPatternMismatch,
}

impl R1Response {
    pub fn new(value: u8) -> Self {
        Self(value)
    }

    pub fn check_error(self) -> Result<R1Response, ResponseError> {
        ensure!(self.is_clear(Self::ILLEGAL_COMMAND), IllegalCommandSnafu);
        ensure!(self.is_clear(Self::COM_CRC_ERROR), ComCrcSnafu);
        ensure!(
            self.is_clear(Self::ERASE_SEQUENCE_ERROR),
            EraseSequenceSnafu
        );
        ensure!(self.is_clear(Self::ADDRESS_ERROR), AddressSnafu);
        ensure!(self.is_clear(Self::PARAMETER_ERROR), ParameterSnafu);

        Ok(self)
    }

    // TODO: remove this when it is no longer needed
    #[allow(dead_code)]
    pub fn response_truncated(self) -> bool {
        self.is_set(Self::ILLEGAL_COMMAND) || self.is_set(Self::COM_CRC_ERROR)
    }

    fn is_clear(self, rhs: Self) -> bool {
        (self & rhs) == Self::NONE
    }

    fn is_set(self, rhs: Self) -> bool {
        (self & rhs) != Self::NONE
    }
}

impl Response for R1Response {
    type ExtraBytes = [u8; 0];

    fn create(r1: R1Response, _extra_bytes: &Self::ExtraBytes) -> Self {
        r1
    }

    fn r1(&self) -> &R1Response {
        self
    }
}

impl R3Response {
    pub fn new(byte2: u8, byte3: u8, byte4: u8, byte5: u8, r1: R1Response) -> Self {
        let b2: u32 = byte2 as u32;
        let b3: u32 = byte3 as u32;
        let b4: u32 = byte4 as u32;
        let b5: u32 = byte5 as u32;

        R3Response((b2 << 24) | (b3 << 16) | (b4 << 8) | b5, r1)
    }

    pub fn card_capacity(&self) -> CardCapacity {
        const CSS: u32 = 0b0100_0000_0000_0000_0000_0000_0000_0000;

        if self.0 & CSS == 0 {
            CardCapacity::Standard
        } else {
            CardCapacity::HighOrExtended
        }
    }
}

impl Response for R3Response {
    type ExtraBytes = [u8; 4];

    fn create(r1: R1Response, extra_bytes: &Self::ExtraBytes) -> Self {
        R3Response::new(
            extra_bytes[0],
            extra_bytes[1],
            extra_bytes[2],
            extra_bytes[3],
            r1,
        )
    }

    fn r1(&self) -> &R1Response {
        &self.1
    }
}

impl R7Response {
    pub fn new(byte2: u8, byte3: u8, byte4: u8, byte5: u8, r1: R1Response) -> Self {
        let b2: u32 = byte2 as u32;
        let b3: u32 = byte3 as u32;
        let b4: u32 = byte4 as u32;
        let b5: u32 = byte5 as u32;

        R7Response((b2 << 24) | (b3 << 16) | (b4 << 8) | b5, r1)
    }

    pub fn check(&self, check_pattern: u8) -> Result<(), ResponseError> {
        const VOLTAGE_ACCEPTED_MASK: u32 = 0b0000_1111 << 8;
        const CHECK_PATTERN_MASK: u32 = 0x0000_00FF;
        ensure!(
            (self.0 & VOLTAGE_ACCEPTED_MASK) >> 8 == VOLTAGE_2_7_TO_3_6.into(),
            UnexpectVoltageSnafu
        );
        ensure!(
            self.0 & CHECK_PATTERN_MASK == check_pattern.into(),
            CheckPatternMismatchSnafu
        );

        Ok(())
    }
}

impl Response for R7Response {
    type ExtraBytes = [u8; 4];

    fn create(r1: R1Response, extra_bytes: &Self::ExtraBytes) -> Self {
        R7Response::new(
            extra_bytes[0],
            extra_bytes[1],
            extra_bytes[2],
            extra_bytes[3],
            r1,
        )
    }

    fn r1(&self) -> &R1Response {
        &self.1
    }
}

// This set of constants is desiged to be all of the specificed values, whether
// they are used in this crate or not. This is taked from section 7.3.2.1 of
// the Simplified Specification.
#[allow(dead_code)]
impl R1Response {
    pub const IDLE: R1Response = R1Response(0b0000_0001);
    pub const ERASE_RESET: R1Response = R1Response(0b0000_0010);
    pub const ILLEGAL_COMMAND: R1Response = R1Response(0b0000_0100);
    pub const COM_CRC_ERROR: R1Response = R1Response(0b0000_1000);
    pub const ERASE_SEQUENCE_ERROR: R1Response = R1Response(0b0001_0000);
    pub const ADDRESS_ERROR: R1Response = R1Response(0b0010_0000);
    pub const PARAMETER_ERROR: R1Response = R1Response(0b0100_0000);

    /// ALL_ERROR is all of the error values. Everything except IDLE
    /// and ERASE_RESET.
    pub const ALL_ERROR: R1Response = R1Response(0b0111_1100);

    pub const NONE: R1Response = R1Response(0);
    pub const ALL: R1Response = R1Response(0b0111_1111);
}

impl BitAnd for R1Response {
    type Output = R1Response;

    fn bitand(self, rhs: Self) -> Self::Output {
        R1Response(self.0 & rhs.0)
    }
}

impl BitAndAssign for R1Response {
    fn bitand_assign(&mut self, rhs: Self) {
        *self = *self & rhs
    }
}

impl BitOr for R1Response {
    type Output = R1Response;

    fn bitor(self, rhs: Self) -> Self::Output {
        R1Response(self.0 | rhs.0)
    }
}

impl BitOrAssign for R1Response {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = *self | rhs
    }
}

impl BitXor for R1Response {
    type Output = R1Response;

    fn bitxor(self, rhs: Self) -> Self::Output {
        R1Response(self.0 ^ rhs.0)
    }
}

impl BitXorAssign for R1Response {
    fn bitxor_assign(&mut self, rhs: Self) {
        *self = *self ^ rhs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r1_error_bit_is_error() {
        let result = R1Response::new(0b0000_0100).check_error();

        assert_eq!(result, Err(ResponseError::IllegalCommand))
    }

    #[test]
    fn r1_multi_error_is_lowest_bit() {
        let result = R1Response::new(0b0001_0100).check_error();

        assert_eq!(result, Err(ResponseError::IllegalCommand))
    }

    #[test]
    fn r1_illegal_command_is_truncated() {
        let r1 = R1Response::new(0b0000_0100);

        assert!(r1.response_truncated(), "r1 unexpectedly not trucated");
    }

    #[test]
    fn r1_parameter_error_not_tructated() {
        let r1 = R1Response::new(0b0100_0000);

        assert!(!r1.response_truncated(), "r1 unexpectedly truncated");
    }

    #[test]
    fn r7_with_unexpect_voltage_is_error() {
        let r7 = R7Response::new(0, 0, 0b0000_0100, 0, R1Response(0));
        let result = r7.check(0);

        assert_eq!(result, Err(ResponseError::UnexpectVoltage));
    }

    #[test]
    fn r7_with_unexpted_check_pattern_is_error() {
        let r7 = R7Response::new(0, 0, VOLTAGE_2_7_TO_3_6, 0xff, R1Response(0));
        let result = r7.check(0xab);

        assert_eq!(result, Err(ResponseError::CheckPatternMismatch));
    }

    #[test]
    fn r7_with_expected_values_is_ok() {
        let check_pattern = 42;

        let r7 = R7Response::new(0, 0, VOLTAGE_2_7_TO_3_6, check_pattern, R1Response(0));
        let result = r7.check(check_pattern);

        assert_eq!(result, Ok(()));
    }

    #[test]
    fn r3_with_ccs_set_gives_expected_capacity() {
        let r3 = R3Response::new(0b0100_0000, 0, 0, 0, R1Response(0));

        assert_eq!(r3.card_capacity(), CardCapacity::HighOrExtended);
    }

    #[test]
    fn r3_with_css_unset_gives_expected_capacity() {
        let r3 = R3Response::new(0, 0, 0, 0, R1Response(0));

        assert_eq!(r3.card_capacity(), CardCapacity::Standard);
    }
}
