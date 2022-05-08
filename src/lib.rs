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

use core::marker::PhantomData;

use embedded_storage::{ReadStorage, Storage};
use snafu::prelude::*;

/// An SD Card interface built from an SPI periferal and a Chip Select pin.
///
/// We need the Chip Select to be separate so we can write some bytes without
/// Chip Select asserted to put the card into SPI mode.
pub struct SDCard<SPI, CS> {
    _cs: PhantomData<CS>,
    _spi: PhantomData<SPI>,
}

impl<SPI, CS> SDCard<SPI, CS> {
    /// Create a new [`SDCard`] using the given `SPI` interface and chip select.
    pub fn new(_spi: SPI, _cs: CS) -> Self {
        Self {
            _cs: PhantomData,
            _spi: PhantomData,
        }
    }
}

/// The error type for [`SDCard`] operations.
#[derive(Debug, Snafu)]
pub struct Error {}

impl<SPI, CS> Storage for SDCard<SPI, CS> {
    fn write(&mut self, _offset: u32, _bytes: &[u8]) -> Result<(), Self::Error> {
        todo!();
    }
}

impl<SPI, CS> ReadStorage for SDCard<SPI, CS> {
    type Error = Error;

    fn read(&mut self, _offset: u32, _bytes: &mut [u8]) -> Result<(), Self::Error> {
        todo!()
    }

    fn capacity(&self) -> usize {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
