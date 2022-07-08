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
mod common;
mod resp;
mod transactions;

#[cfg(test)]
mod testutils;

use core::fmt::Debug;

use common::CardCapacity;
use embedded_hal::{
    blocking::{
        delay::DelayUs,
        spi::{Transfer, Write},
    },
    digital::v2::OutputPin,
};
use embedded_storage::{ReadStorage, Storage};
use snafu::{prelude::*, IntoError};
use transactions::{initilization_flow, power_up_card, with_cs_low};

/// An SD Card interface built from an SPI periferal and a Chip Select pin.
///
/// We need the Chip Select to be separate so we can write some bytes without
/// Chip Select asserted to put the card into SPI mode.
pub struct SDCard<SPI, CS, DELAY> {
    spi: SPI,
    cs: CS,
    // TODO: removed this when it is no longer needed
    #[allow(dead_code)]
    delay: DELAY,
    // TODO: removed this when it is no longer needed
    #[allow(dead_code)]
    capacity: CardCapacity,
}

impl<SPI, CS, DELAY> SDCard<SPI, CS, DELAY>
where
    SPI: Debug + Write<u8> + Transfer<u8>,
    CS: Debug + OutputPin,
    DELAY: DelayUs<u16>,
{
    /// Create a new [`SDCard`] using the given `SPI` interface and chip select.
    ///
    /// The `SPI` interface should have a clock rate between 100 kHz and 400 kHz.
    /// See [`SDCard::with_speed_increase`] for a means to increase the clock
    /// rate after the card initilization is complete.
    pub fn new(spi: SPI, cs: CS, delay: DELAY) -> Result<Self, InitilizationError<SPI, CS>> {
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
        mut spi: SPI,
        mut cs: CS,
        mut delay: DELAY,
        increase_speed: impl FnOnce(SPI) -> SPI,
    ) -> Result<Self, InitilizationError<SPI, CS>> {
        // This initialized the SD card using the power up sequence in section
        // 6.4.1 followed by the initilization flow from Figure 7-2. (Unless
        // otherwise indicated the section and figure refences in the comments
        // are references to the Simplifed Specification).

        let result = power_up_card(&mut spi, &mut cs, &mut delay)
            .and_then(|_| with_cs_low(&mut cs, &mut spi, initilization_flow));

        match result {
            Ok(capacity) => {
                // 8. (optional) Increase frequency of the SPI
                let spi = increase_speed(spi);
                Ok(Self {
                    cs,
                    spi,
                    capacity,
                    delay,
                })
            }
            Err(e) => Err(InitilizationSnafu { cs, spi }.into_error(e)),
        }
    }
}

impl<SPI, CS, DELAY> SDCard<SPI, CS, DELAY> {
    /// Consume the `SDCard` and return the underlying `SPI` and chip select.
    pub fn release(self) -> (SPI, CS, DELAY) {
        (self.spi, self.cs, self.delay)
    }
}

/// The error type for [`SDCard`] initilization operations.
#[derive(Debug, Snafu)]
#[snafu(display("Unable to initilize the SD Card in SPI mode."))]
pub struct InitilizationError<SPI: Debug, CS: Debug> {
    source: transactions::Error,
    spi: SPI,
    cs: CS,
}

impl<SPI: Debug, CS: Debug> InitilizationError<SPI, CS> {
    /// Consume the `InitilizationError` and return the `SPI` and chip select
    /// that had been passed to the `SDCard` initilization function.
    pub fn release(self) -> (SPI, CS) {
        (self.spi, self.cs)
    }
}

/// The error type for [`SDCard`] IO operations.
#[derive(Debug, Snafu)]
pub struct IOError {}

impl<SPI, CS, DELAY> Storage for SDCard<SPI, CS, DELAY> {
    fn write(&mut self, _offset: u32, _bytes: &[u8]) -> Result<(), Self::Error> {
        todo!();
    }
}

impl<SPI, CS, DELAY> ReadStorage for SDCard<SPI, CS, DELAY> {
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
    use std::sync::Arc;

    use embedded_hal_mock::delay;

    use crate::testutils::{FakeCard, StubPin};

    use super::*;

    #[test]
    fn sd_card_with_speed_increase_increases_speed() {
        let mut increased = false;
        let delay = delay::MockNoop::new();

        SDCard::with_speed_increase(FakeCard::default(), StubPin, delay, |s| {
            increased = true;
            s
        })
        .expect("error initilizing the card");

        assert!(
            increased,
            "with_speed_increase() did not call the passed closure"
        );
    }

    #[test]
    fn sd_card_release_returns_contained_resourses() {
        let spi = Arc::new(5);
        let cs = Arc::new(true);
        let delay = Arc::new("yes");

        let sut = SDCard {
            spi: spi.clone(),
            cs: cs.clone(),
            delay: delay.clone(),
            capacity: CardCapacity::Standard,
        };
        let (rel_spi, rel_cs, rel_delay) = sut.release();

        assert!(Arc::ptr_eq(&spi, &rel_spi), "spi missmatch on release");
        assert!(Arc::ptr_eq(&cs, &rel_cs), "cs missmatch on release");
        assert!(
            Arc::ptr_eq(&delay, &rel_delay),
            "delay missmatch on release"
        );
    }
}
