// Copyright 2022 Steven Bosnick
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE-2.0 or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms

//! Constants from the Simplified Specificiation that are used in more than
//! one module.

/// Voltage supplied or accepted nibble.
///
/// This is used as the voltage supplied value (VHS) for a SendIfCond command
/// and as the voltage accepted value when interpreting the R7 response that
/// is returned.
///
/// This is taken from Table 4-18 (for voltage supplied) or Table 4-41
///(for voltage accepted) in the Simplified Specificiation.
pub const VOLTAGE_2_7_TO_3_6: u8 = 0b0001;

/// The check pattern we use for a SendIfCond command and expect to be echoed
/// back in the R7 response.
///
/// This could be any value but this the one we picked.
pub const IF_COND_CHECK_PATTERN: u8 = 0b0101_0101;

/// The card capacity classification from section 3.3.2.
///
/// Note that Ultra Capacity (SDUC) cards are not supported in SPI mode
/// (see section 7.1) so there is no entry for them here.
#[derive(Debug, PartialEq)]
pub enum CardCapacity {
    /// SDSC card
    Standard,

    /// SDHC or SDXC card
    HighOrExtended,
}
