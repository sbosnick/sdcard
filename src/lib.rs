// Copyright 2022 Steven Bosnick
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE-2.0 or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms

//! An embedded-hal driver for an SDCard over SPI.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![deny(missing_docs, warnings)]

mod cmds;
mod constants;
mod resp;

use embedded_hal::blocking::delay::DelayMs;
use embedded_storage::{ReadStorage, Storage};
use snafu::prelude::*;

/// An SD Card interface built from an SPI periferal and a Chip Select pin.
///
/// We need the Chip Select to be separate so we can write some bytes without
/// Chip Select asserted to put the card into SPI mode.
pub struct SDCard<SPI, CS> {
    _cs: CS,
    _spi: SPI,
}

impl<SPI, CS> SDCard<SPI, CS> {
    /// Create a new [`SDCard`] using the given `SPI` interface and chip select.
    ///
    /// The `SPI` interface should have a clock rate between 100 kHz and 400 kHz.
    /// See [`SDCard::with_speed_increase`] for a means to increase the clock
    /// rate after the card initilization is complete.
    pub fn new(spi: SPI, cs: CS, delay: &mut impl DelayMs<u8>) -> Result<Self, InitilizationError> {
        Self::with_speed_increase(spi, cs, delay, |spi| spi)
    }

    /// Create a new [`SDCard`] using the given `SPI` interface and chip select.
    ///
    /// The `SPI` interface should have a clock rate between 100 kHz and 400 kHz.
    /// After the SD card has been initialized, the clock rate on the `SPI`
    /// interface can be increased through the supplied `increase_speed` closure.
    /// The speed should be increased to 25 MHz (the maximum speed for an SD card
    /// using `SPI` mode).
    pub fn with_speed_increase(
        spi: SPI,
        cs: CS,
        _delay: &mut impl DelayMs<u8>,
        increase_speed: impl FnOnce(SPI) -> SPI,
    ) -> Result<Self, InitilizationError> {
        // This initialized the SD card using the power up sequence in section
        // 6.4.1 followed by the initilization flow from Figure 7-2. (Unless
        // otherwise indicated the section and figure refences in the comments
        // are references to the Simplifed Specification).

        // 1. delay 1 ms then 74 clocks with CS high (6.4.1.1)
        // 2. GoToIdle
        // 3. SendIfCond and check for illegal command (v1 card)
        // 4. CrcOnOff to turn crc checking on
        // 5. ReadOcr and check for compatible voltage (or assume it is in range)
        // 6. SendOpCond (with HCR if not v1 card) repeatedly until not idle
        // 7. If not v1 card then ReadOcr and check card capacity
        // 8. (optional) Increase frequency of the SPI

        Ok(Self {
            _cs: cs,
            _spi: increase_speed(spi),
        })
    }
}

/// The error type for [`SDCard`] initilization operations.
#[derive(Debug, Snafu)]
pub struct InitilizationError {}

/// The error type for [`SDCard`] IO operations.
#[derive(Debug, Snafu)]
pub struct IOError {}

impl<SPI, CS> Storage for SDCard<SPI, CS> {
    fn write(&mut self, _offset: u32, _bytes: &[u8]) -> Result<(), Self::Error> {
        todo!();
    }
}

impl<SPI, CS> ReadStorage for SDCard<SPI, CS> {
    type Error = IOError;

    fn read(&mut self, _offset: u32, _bytes: &mut [u8]) -> Result<(), Self::Error> {
        todo!()
    }

    fn capacity(&self) -> usize {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use embedded_hal_mock::delay;

    use super::*;

    #[test]
    fn sd_card_with_speed_increase_increases_speed() {
        let mut increased = false;
        let mut delay = delay::MockNoop::new();

        let _ = SDCard::with_speed_increase((), (), &mut delay, |s| {
            increased = true;
            s
        });

        assert!(
            increased,
            "with_speed_increase() did not call the passed closure"
        );
    }
}
