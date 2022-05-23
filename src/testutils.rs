// Copyright 2022 Steven Bosnick
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE-2.0 or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms

//! Utilities to support tests.

use embedded_hal::{blocking::spi::Write, digital::v2::OutputPin};

#[derive(Debug)]
pub struct StubSpi;
#[derive(Debug)]
pub struct StubPin;
pub struct StubError;

impl OutputPin for StubPin {
    type Error = StubError;

    fn set_low(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl Write<u8> for StubSpi {
    type Error = StubError;

    fn write(&mut self, _words: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }
}
