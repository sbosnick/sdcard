// Copyright 2022 Steven Bosnick
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE-2.0 or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms

//! Functions and types related to transactions with an SD Card over SPI.
//!
//! The transactions include both those related to initilization and those
//! related to data transfer (after initilization).

use embedded_hal::{
    blocking::{
        delay::DelayMs,
        spi::{Transfer, Write},
    },
    digital::v2::OutputPin,
};
use snafu::prelude::*;

use crate::{
    cmds,
    resp::{R1Response, ResponseError},
};

const WAIT_FOR_CARD_COUNT: u32 = 32_000;
const MAX_WAIT_FOR_RESPONSE: u32 = 8;

#[derive(Debug, PartialEq, Snafu)]
pub enum Error {
    #[snafu(display("Unable to set chip select state for SPI."))]
    ChipSelect,

    #[snafu(display("Unable to write to SPI."))]
    SpiWrite,

    #[snafu(display("Unable to transfer to and from SPI."))]
    SpiTransfer,

    #[snafu(display("Timeout waiting for the card to be ready."))]
    WaitForCardTimeout,

    #[snafu(display("Timeout waiting for the card to respond to a command."))]
    WaitForResponseTimeout,

    #[snafu(display("The response to a command indicated an error."))]
    CommandResponse { source: ResponseError },
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

pub fn initilization_flow<SPI>(spi: &mut SPI) -> Result<(), Error>
where
    SPI: Write<u8> + Transfer<u8>,
{
    let mut command = [0; 6];

    // 2. GoIdleState
    cmds::go_idle_state(&mut command);
    execute_command(spi, &command)?;
    // 3. SendIfCond and check for illegal command (v1 card)
    // 4. CrcOnOff to turn crc checking on
    // 5. ReadOcr and check for compatible voltage (or assume it is in range)
    // 6. SendOpCond (with HCR if not v1 card) repeatedly until not idle
    // 7. If not v1 card then ReadOcr and check card capacity

    // TODO: implement this
    Ok(())
}

pub fn with_cs_low<CS, SPI, F, O>(cs: &mut CS, spi: &mut SPI, f: F) -> Result<O, Error>
where
    CS: OutputPin,
    SPI: Write<u8>,
    F: Fn(&mut SPI) -> Result<O, Error>,
{
    cs.set_low()
        .map_err(|_| ChipSelectSnafu {}.build())
        .and_then(|_| f(spi))
        .and_then(|o| {
            cs.set_high()
                .map(|_| o)
                .map_err(|_| ChipSelectSnafu {}.build())
        })
        .or_else(|e| {
            // ignore the error to give priority to the error from f(spi)
            let _ = cs.set_high();
            Err(e)
        })
}

// TODO: remove this when it is no longer needed
#[allow(dead_code)]
fn execute_command<SPI>(spi: &mut SPI, cmd: &[u8]) -> Result<R1Response, Error>
where
    SPI: Write<u8> + Transfer<u8>,
{
    debug_assert_eq!(cmd.len(), 6);

    wait_for_card(spi)?;

    spi.write(cmd).map_err(|_| SpiWriteSnafu {}.build())?;

    for _ in 0..MAX_WAIT_FOR_RESPONSE {
        let recv = receive(spi)?;
        if recv != 0xff {
            return Ok(R1Response::new(recv)
                .check_error()
                .context(CommandResponseSnafu {})?);
        }
    }

    WaitForResponseTimeoutSnafu {}.fail()
}

fn wait_for_card<SPI: Transfer<u8>>(spi: &mut SPI) -> Result<(), Error> {
    for _ in 0..WAIT_FOR_CARD_COUNT {
        if receive(spi)? == 0xff {
            return Ok(());
        }

        // TODO: use a DelayUs impl here
    }

    WaitForCardTimeoutSnafu {}.fail()
}

fn receive<SPI: Transfer<u8>>(spi: &mut SPI) -> Result<u8, Error> {
    let mut buffer = [0xff];
    let response = spi
        .transfer(&mut buffer)
        .map_err(|_| SpiTransferSnafu {}.build())?;

    Ok(response[0])
}

#[cfg(test)]
mod test {
    use std::{io::ErrorKind, iter};

    use crate::testutils::StubSpi;

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

    #[test]
    fn with_cs_low_toggles_cs() {
        let set_low = pin::Transaction::set(pin::State::Low);
        let set_high = pin::Transaction::set(pin::State::High);
        let mut cs = pin::Mock::new(&[set_low, set_high]);

        let _ = with_cs_low(&mut cs, &mut StubSpi, |_| Ok(()));

        cs.done();
    }

    #[test]
    fn wait_for_card_is_ok_after_cipo_high() {
        let expected = [
            spi::Transaction::transfer(vec![0xff], vec![0x00]),
            spi::Transaction::transfer(vec![0xff], vec![0x00]),
            spi::Transaction::transfer(vec![0xff], vec![0x00]),
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
        ];
        let mut spi = spi::Mock::new(&expected);

        let result = wait_for_card(&mut spi);

        spi.done();
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn wait_for_card_is_error_after_too_much_cipo_low() {
        let mut spi = spi::Mock::new(
            iter::repeat(&spi::Transaction::transfer(vec![0xff], vec![0x00]))
                .take(WAIT_FOR_CARD_COUNT.try_into().unwrap()),
        );

        let result = wait_for_card(&mut spi);

        assert_eq!(result, Err(Error::WaitForCardTimeout));
    }

    #[test]
    fn execute_command_writes_command() {
        let command = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
        let expectations = [
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::write(command.clone()),
            spi::Transaction::transfer(vec![0xff], vec![0x00]),
        ];
        let mut spi = spi::Mock::new(&expectations);

        execute_command(&mut spi, &command).expect("error executing command");

        spi.done();
    }

    #[test]
    fn execute_command_with_error_response_is_error() {
        let command = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
        let expectations = [
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::write(command.clone()),
            spi::Transaction::transfer(vec![0xff], vec![0b0100_0000]),
        ];
        let mut spi = spi::Mock::new(&expectations);

        let result = execute_command(&mut spi, &command);

        spi.done();
        assert!(matches!(result, Err(Error::CommandResponse { source: _ })));
    }

    #[test]
    fn execute_command_with_no_response_times_out() {
        let command = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
        let expectations = [
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::write(command.clone()),
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
        ];
        let mut spi = spi::Mock::new(&expectations);

        let result = execute_command(&mut spi, &command);

        spi.done();
        assert!(matches!(result, Err(Error::WaitForResponseTimeout)));
    }
}
