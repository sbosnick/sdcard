// Copyright 2022 Steven Bosnick
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE-2.0 or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms

//! Utilities to support tests.

use embedded_hal::{
    blocking::spi::{Transfer, Write},
    digital::v2::OutputPin,
};

use crate::common;

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

impl Transfer<u8> for StubSpi {
    type Error = StubError;

    fn transfer<'w>(&mut self, words: &'w mut [u8]) -> Result<&'w [u8], Self::Error> {
        Ok(words)
    }
}

#[derive(Debug, Default)]
pub struct FakeCard {
    state: State,
}

impl Write<u8> for FakeCard {
    type Error = StubError;

    fn write(&mut self, words: &[u8]) -> Result<(), Self::Error> {
        match self.state {
            State::Start => Ok(()),
            State::CommandPending if words[0] & 0b1100_0000 == 0b0100_0000 => {
                if words[0] & 0b0011_1111 == 8 {
                    self.state = State::R7ResponsePending(4);
                } else {
                    self.state = State::ResponsePending;
                }
                Ok(())
            }
            State::CommandPending => todo!(),
            State::ResponsePending => todo!(),
            State::R7ResponsePending(_) => todo!(),
        }
    }
}

impl Transfer<u8> for FakeCard {
    type Error = StubError;

    fn transfer<'w>(&mut self, words: &'w mut [u8]) -> Result<&'w [u8], Self::Error> {
        match self.state {
            State::Start if words[0] == 0xff => {
                self.state = State::CommandPending;
                Ok(words)
            }
            State::Start => Err(StubError),
            State::CommandPending => todo!(),
            State::ResponsePending => {
                self.state = State::Start;
                // Note: this is a non-idle, non-error R1 response
                words[0] = 0;
                Ok(words)
            }
            State::R7ResponsePending(byte) => {
                self.state = if byte == 0 {
                    State::Start
                } else {
                    State::R7ResponsePending(byte - 1)
                };
                words[0] = match byte {
                    4 => 0,
                    3 => 0,
                    2 => 0,
                    1 => common::VOLTAGE_2_7_TO_3_6,
                    0 => common::IF_COND_CHECK_PATTERN,
                    _ => panic!("unexpect byte count for R7 response"),
                };
                Ok(words)
            }
        }
    }
}

#[derive(Debug)]
enum State {
    Start,
    CommandPending,
    ResponsePending,
    R7ResponsePending(u8),
}

impl Default for State {
    fn default() -> Self {
        State::Start
    }
}
