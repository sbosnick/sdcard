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

use core::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign};

use snafu::{ensure, Snafu};

/// Newtype to embodity the logic of an R1 response.
///
/// This type is based on section 7.3.2.1 of the Simplified Specification.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct R1Response(u8);

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
}

impl R1Response {
    // TODO: remove this when it is no longer needed
    #[allow(dead_code)]
    pub fn new(value: u8) -> Self {
        Self(value)
    }

    // TODO: remove this when it is no longer needed
    #[allow(dead_code)]
    pub fn is_idle(self) -> Result<bool, ResponseError> {
        self.check_error().map(|r| r.is_set(Self::IDLE))
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
    fn r1_idle_with_error_is_error() {
        let result = R1Response::new(0b0000_0101).is_idle();

        assert_eq!(result, Err(ResponseError::IllegalCommand))
    }

    #[test]
    fn r1_idle_without_error_is_idle() {
        let result = R1Response::new(0b0000_0001).is_idle();

        assert_eq!(result, Ok(true));
    }

    #[test]
    fn r1_none_is_not_idle_or_error() {
        let result = R1Response::new(0).is_idle();

        assert_eq!(result, Ok(false));
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
}
