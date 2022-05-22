// Copyright 2022 Steven Bosnick
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE-2.0 or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms

//! Functions and types related to initilization of an SD Card over SPI

use embedded_hal::{
    blocking::{delay::DelayMs, spi::Write},
    digital::v2::OutputPin,
};
use snafu::prelude::*;

#[derive(Debug, PartialEq, Snafu)]
pub enum Error {
    #[snafu(display("Unable to set chip select state for SPI initilization."))]
    ChipSelect,

    #[snafu(display("Unable to write to SPI for initilization."))]
    SpiWrite,
}

/// Power up sequence from section 6.4.1 of the Simplified Specification.
pub fn power_up_card(
    spi: &mut impl Write<u8>,
    cs: &mut impl OutputPin,
    delay: &mut impl DelayMs<u8>,
) -> Result<(), Error> {
    // 1. delay 1 ms then 74 clocks with CS high (6.4.1.1)

    delay.delay_ms(1);
    cs.set_high().map_err(|_| ChipSelectSnafu {}.build())?;

    // Note that 74 bits rounded up is 10 bytes
    spi.write(&[0xff; 10])
        .map_err(|_| SpiWriteSnafu {}.build())?;

    Ok(())
}

#[cfg(test)]
mod test {
    use std::io::ErrorKind;

    use embedded_hal_mock::{delay, pin, spi, MockError};

    use super::*;

    #[test]
    fn power_up_card_has_74_clocks_with_cs_high() {
        let mut spi = spi::Mock::new(&[spi::Transaction::write([0xff; 10].to_vec())]);
        let mut cs = pin::Mock::new(&[pin::Transaction::set(pin::State::High)]);
        let mut delay = delay::MockNoop::new();

        power_up_card(&mut spi, &mut cs, &mut delay).expect("Unable to power up");

        spi.done();
        cs.done();
    }

    #[test]
    fn power_up_card_handles_cs_high_error() {
        let go_high = pin::Transaction::set(pin::State::High)
            .with_error(MockError::Io(ErrorKind::Unsupported));
        let mut spi = spi::Mock::new(&[spi::Transaction::write([0xff; 10].to_vec())]);
        let mut cs = pin::Mock::new(&[go_high]);
        let mut delay = delay::MockNoop::new();

        let result = power_up_card(&mut spi, &mut cs, &mut delay);

        assert_eq!(result, Err(Error::ChipSelect));
    }
}
