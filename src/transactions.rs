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
        delay::DelayUs,
        spi::{Transfer, Write},
    },
    digital::v2::OutputPin,
};
use snafu::prelude::*;

use crate::{
    cmds::{self, HostCapacitySupport},
    common::{self, CardCapacity},
    resp::{R1Response, R3Response, R7Response, Response, ResponseError},
};

const WAIT_FOR_CARD_COUNT: u32 = 32;
const WAIT_FOR_CARD_DELAY: u16 = 10;
const MAX_WAIT_FOR_RESPONSE: u32 = 8;
const MAX_IF_COND_COUNT: u32 = 5;
const MAX_OP_COND_COUNT: u32 = 3_200;
const OP_COND_DELAY: u16 = 50;

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

    #[snafu(display("The SD card cannot be initilizationed and is unusable."))]
    UnusableCard,
}

/// Power up sequence from section 6.4.1 of the Simplified Specification.
pub fn power_up_card(
    spi: &mut impl Write<u8>,
    cs: &mut impl OutputPin,
    delay: &mut impl DelayUs<u16>,
) -> Result<(), Error> {
    // 1. delay 1 ms then 74 clocks with CS high (6.4.1.1)

    delay.delay_us(1000);
    cs.set_high().map_err(|_| ChipSelectSnafu {}.build())?;

    // Note that 74 bits rounded up is 10 bytes
    spi.write(&[0xff; 10])
        .map_err(|_| SpiWriteSnafu {}.build())?;

    Ok(())
}

pub fn initilization_flow<SPI, DELAY>(
    spi: &mut SPI,
    delay: &mut DELAY,
) -> Result<CardCapacity, Error>
where
    SPI: Write<u8> + Transfer<u8>,
    DELAY: DelayUs<u16>,
{
    let mut command = [0; 6];

    // 2. GoIdleState
    cmds::go_idle_state(&mut command);
    execute_command(spi, delay, &command)?;

    // 3. SendIfCond and check for illegal command (v1 card)
    let version = send_if_cond(spi, delay)?;

    // 4. CrcOnOff to turn crc checking on
    cmds::crc_on_off(cmds::CrcOption::On, &mut command);
    execute_command(spi, delay, &command)?;

    // 5. ReadOcr and check for compatible voltage (or assume it is in range)
    // For now assume that the voltage is 3.3 V which is always supported.

    // 6. SendOpCond (with HCR if not v1 card) repeatedly until not idle
    send_op_cond(spi, version, delay)?;

    // 7. If not v1 card then ReadOcr and check card capacity
    check_card_capacity(spi, delay, version)
}

pub fn with_cs_low<CS, SPI, DELAY, F, O>(
    cs: &mut CS,
    spi: &mut SPI,
    delay: &mut DELAY,
    f: F,
) -> Result<O, Error>
where
    CS: OutputPin,
    SPI: Write<u8>,
    DELAY: DelayUs<u16>,
    F: Fn(&mut SPI, &mut DELAY) -> Result<O, Error>,
{
    cs.set_low()
        .map_err(|_| ChipSelectSnafu {}.build())
        .and_then(|_| f(spi, delay))
        .and_then(|o| {
            cs.set_high()
                .map(|_| o)
                .map_err(|_| ChipSelectSnafu {}.build())
        })
        .map_err(|e| {
            // ignore the error to give priority to the error from f(spi)
            let _ = cs.set_high();
            e
        })
}

fn send_if_cond<SPI, DELAY>(spi: &mut SPI, delay: &mut DELAY) -> Result<Version, Error>
where
    SPI: Write<u8> + Transfer<u8>,
    DELAY: DelayUs<u16>,
{
    let mut command = [0; 6];
    let check_pattern = common::IF_COND_CHECK_PATTERN;

    for _ in 0..MAX_IF_COND_COUNT {
        let mut retry = false;

        cmds::send_if_cond(check_pattern, &mut command);
        let result = R7Response::execute_command(spi, delay, &command)
            .map(|r7| {
                if r7.check(check_pattern).is_err() {
                    retry = true;
                }
                Version::V2
            })
            .or_else(|err| match err {
                Error::CommandResponse { source } if source == ResponseError::IllegalCommand => {
                    Ok(Version::V1)
                }
                _ => Err(err),
            });

        if !retry {
            return result;
        }
    }

    UnusableCardSnafu {}.fail()
}

fn send_op_cond<SPI>(
    spi: &mut SPI,
    version: Version,
    delay: &mut impl DelayUs<u16>,
) -> Result<(), Error>
where
    SPI: Write<u8> + Transfer<u8>,
{
    let mut command = [0; 6];

    for _ in 0..MAX_OP_COND_COUNT {
        cmds::app_cmd(&mut command);
        execute_command(spi, delay, &command)?;

        cmds::sd_send_op_cond(version.into(), &mut command);
        let r1 = execute_command(spi, delay, &command)?;

        if r1 & R1Response::IDLE == R1Response::NONE {
            return Ok(());
        }

        delay.delay_us(OP_COND_DELAY)
    }

    UnusableCardSnafu {}.fail()
}

fn check_card_capacity<SPI, DELAY>(
    spi: &mut SPI,
    delay: &mut DELAY,
    version: Version,
) -> Result<CardCapacity, Error>
where
    SPI: Write<u8> + Transfer<u8>,
    DELAY: DelayUs<u16>,
{
    match version {
        Version::V1 => Ok(CardCapacity::Standard),
        Version::V2 => {
            let mut command = [0; 6];

            cmds::read_ocr(&mut command);
            R3Response::execute_command(spi, delay, &command).map(|r3| r3.card_capacity())
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Version {
    V1,
    V2,
}

impl From<Version> for HostCapacitySupport {
    fn from(version: Version) -> Self {
        match version {
            Version::V1 => HostCapacitySupport::ScOnly,
            Version::V2 => HostCapacitySupport::HcOrXcSupported,
        }
    }
}

fn execute_command<SPI, DELAY>(
    spi: &mut SPI,
    delay: &mut DELAY,
    cmd: &[u8],
) -> Result<R1Response, Error>
where
    SPI: Write<u8> + Transfer<u8>,
    DELAY: DelayUs<u16>,
{
    R1Response::execute_command(spi, delay, cmd)
}

trait Execute
where
    Self: Sized,
{
    fn execute_command<SPI, DELAY>(
        spi: &mut SPI,
        delay: &mut DELAY,
        cmd: &[u8],
    ) -> Result<Self, Error>
    where
        SPI: Write<u8> + Transfer<u8>,
        DELAY: DelayUs<u16>;
}

impl<R: Response> Execute for R {
    fn execute_command<SPI, DELAY>(
        spi: &mut SPI,
        delay: &mut DELAY,
        cmd: &[u8],
    ) -> Result<Self, Error>
    where
        SPI: Write<u8> + Transfer<u8>,
        DELAY: DelayUs<u16>,
    {
        debug_assert_eq!(cmd.len(), 6);

        wait_for_card(spi, delay)?;

        spi.write(cmd).map_err(|_| SpiWriteSnafu {}.build())?;

        for _ in 0..MAX_WAIT_FOR_RESPONSE {
            let recv = receive(spi)?;
            if recv != 0xff {
                let r1 = R1Response::new(recv);
                let mut extra = R::ExtraBytes::default();
                if !r1.response_truncated() {
                    for e in extra.as_mut().iter_mut() {
                        *e = receive(spi)?;
                    }
                }

                return r1
                    .check_error()
                    .context(CommandResponseSnafu {})
                    .map(|r1| R::create(r1, &extra));
            }
        }

        WaitForResponseTimeoutSnafu {}.fail()
    }
}

fn wait_for_card<SPI, DELAY>(spi: &mut SPI, delay: &mut DELAY) -> Result<(), Error>
where
    SPI: Transfer<u8>,
    DELAY: DelayUs<u16>,
{
    for _ in 0..WAIT_FOR_CARD_COUNT {
        if receive(spi)? == 0xff {
            return Ok(());
        }

        delay.delay_us(WAIT_FOR_CARD_DELAY);
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

    use crate::{common, testutils::StubSpi};

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
        let mut delay = delay::MockNoop::new();

        let _ = with_cs_low(&mut cs, &mut StubSpi, &mut delay, |_, _| Ok(()));

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
        let mut delay = delay::MockNoop::new();

        let result = wait_for_card(&mut spi, &mut delay);

        spi.done();
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn wait_for_card_is_error_after_too_much_cipo_low() {
        let mut spi = spi::Mock::new(
            iter::repeat(&spi::Transaction::transfer(vec![0xff], vec![0x00]))
                .take(WAIT_FOR_CARD_COUNT.try_into().unwrap()),
        );
        let mut delay = delay::MockNoop::new();

        let result = wait_for_card(&mut spi, &mut delay);

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
        let mut delay = delay::MockNoop::new();

        execute_command(&mut spi, &mut delay, &command).expect("error executing command");

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
        let mut delay = delay::MockNoop::new();

        let result = execute_command(&mut spi, &mut delay, &command);

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
        let mut delay = delay::MockNoop::new();

        let result = execute_command(&mut spi, &mut delay, &command);

        spi.done();
        assert!(matches!(result, Err(Error::WaitForResponseTimeout)));
    }

    #[test]
    fn r7_execute_command_does_not_recv_extra_bytes_for_truncated_r1() {
        let command = vec![0b0100_1000, 0, 0, common::VOLTAGE_2_7_TO_3_6, 85, 117];
        let expectations = [
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::write(command.clone()),
            spi::Transaction::transfer(vec![0xff], vec![0b0000_0100]), // R1 with illegal command
        ];
        let mut spi = spi::Mock::new(&expectations);
        let mut delay = delay::MockNoop::new();

        let _result = R7Response::execute_command(&mut spi, &mut delay, &command);

        spi.done();
    }

    #[test]
    fn r7_exectute_command_recv_extra_bytes_for_non_error_r1() {
        let command = vec![0b0100_1000, 0, 0, common::VOLTAGE_2_7_TO_3_6, 85, 117];
        let expectations = [
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::write(command.clone()),
            spi::Transaction::transfer(vec![0xff], vec![0b0000_0000]),
            spi::Transaction::transfer(vec![0xff], vec![0x00]),
            spi::Transaction::transfer(vec![0xff], vec![0x00]),
            spi::Transaction::transfer(vec![0xff], vec![0x00]),
            spi::Transaction::transfer(vec![0xff], vec![0x00]),
        ];
        let mut spi = spi::Mock::new(&expectations);
        let mut delay = delay::MockNoop::new();

        let _result = R7Response::execute_command(&mut spi, &mut delay, &command);

        spi.done();
    }

    #[test]
    fn r7_execute_command_recv_extra_bytes_for_non_truncate_error_r1() {
        let command = vec![0b0100_1000, 0, 0, common::VOLTAGE_2_7_TO_3_6, 85, 117];
        let expectations = [
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::write(command.clone()),
            spi::Transaction::transfer(vec![0xff], vec![0b0100_0000]), // R1 with parameter error
            spi::Transaction::transfer(vec![0xff], vec![0x00]),
            spi::Transaction::transfer(vec![0xff], vec![0x00]),
            spi::Transaction::transfer(vec![0xff], vec![0x00]),
            spi::Transaction::transfer(vec![0xff], vec![0x00]),
        ];
        let mut spi = spi::Mock::new(&expectations);
        let mut delay = delay::MockNoop::new();

        let _result = R7Response::execute_command(&mut spi, &mut delay, &command);

        spi.done();
    }

    #[test]
    fn send_if_cond_illegal_command_is_v1() {
        let command = vec![0b0100_1000, 0, 0, common::VOLTAGE_2_7_TO_3_6, 85, 117];
        let expectations = [
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::write(command),
            spi::Transaction::transfer(vec![0xff], vec![0b0000_0100]), // R1 with illegal command
        ];
        let mut spi = spi::Mock::new(&expectations);
        let mut delay = delay::MockNoop::new();

        let result = send_if_cond(&mut spi, &mut delay);

        spi.done();
        assert!(matches!(result, Ok(Version::V1)));
    }

    #[test]
    fn send_if_cond_with_valid_r7_is_v2() {
        let command = vec![0b0100_1000, 0, 0, common::VOLTAGE_2_7_TO_3_6, 85, 117];
        let expectations = [
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::write(command),
            spi::Transaction::transfer(vec![0xff], vec![0]), // R1 (R7 byte 1)
            spi::Transaction::transfer(vec![0xff], vec![0]), // R7 byte 2
            spi::Transaction::transfer(vec![0xff], vec![0]), // R7 byte 3
            spi::Transaction::transfer(vec![0xff], vec![common::VOLTAGE_2_7_TO_3_6]), // R7 byte 4
            spi::Transaction::transfer(vec![0xff], vec![85]), // R7 byte 5
        ];
        let mut spi = spi::Mock::new(&expectations);
        let mut delay = delay::MockNoop::new();

        let result = send_if_cond(&mut spi, &mut delay);

        spi.done();
        assert!(matches!(result, Ok(Version::V2)));
    }

    #[test]
    fn send_if_cond_with_valid_r7_on_second_try_is_v2() {
        let command = vec![0b0100_1000, 0, 0, common::VOLTAGE_2_7_TO_3_6, 85, 117];
        let expectations = [
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::write(command.clone()),
            spi::Transaction::transfer(vec![0xff], vec![0]), // R1 (R7 byte 1)
            spi::Transaction::transfer(vec![0xff], vec![0]), // R7 byte 2
            spi::Transaction::transfer(vec![0xff], vec![0]), // R7 byte 3
            spi::Transaction::transfer(vec![0xff], vec![common::VOLTAGE_2_7_TO_3_6]), // R7 byte 4
            spi::Transaction::transfer(vec![0xff], vec![12]), // R7 byte 5
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::write(command),
            spi::Transaction::transfer(vec![0xff], vec![0]), // R1 (R7 byte 1)
            spi::Transaction::transfer(vec![0xff], vec![0]), // R7 byte 2
            spi::Transaction::transfer(vec![0xff], vec![0]), // R7 byte 3
            spi::Transaction::transfer(vec![0xff], vec![common::VOLTAGE_2_7_TO_3_6]), // R7 byte 4
            spi::Transaction::transfer(vec![0xff], vec![85]), // R7 byte 5
        ];
        let mut spi = spi::Mock::new(&expectations);
        let mut delay = delay::MockNoop::new();

        let result = send_if_cond(&mut spi, &mut delay);

        spi.done();
        assert!(matches!(result, Ok(Version::V2)));
    }

    #[test]
    fn send_if_cond_with_repeated_invalid_r7_is_unusable() {
        let check_pattern = common::IF_COND_CHECK_PATTERN;
        let not_check_pattern = check_pattern + 5;
        let command = vec![
            0b0100_1000,
            0,
            0,
            common::VOLTAGE_2_7_TO_3_6,
            check_pattern,
            117,
        ];
        let mut expectations = Vec::new();
        for _ in 0..MAX_IF_COND_COUNT {
            expectations.extend([
                spi::Transaction::transfer(vec![0xff], vec![0xff]),
                spi::Transaction::write(command.clone()),
                spi::Transaction::transfer(vec![0xff], vec![0]), // R1 (R7 byte 1)
                spi::Transaction::transfer(vec![0xff], vec![0]), // R7 byte 2
                spi::Transaction::transfer(vec![0xff], vec![0]), // R7 byte 3
                spi::Transaction::transfer(vec![0xff], vec![common::VOLTAGE_2_7_TO_3_6]), // R7 byte 4
                spi::Transaction::transfer(vec![0xff], vec![not_check_pattern]), // R7 byte 5
            ]);
        }
        let mut spi = spi::Mock::new(&expectations);
        let mut delay = delay::MockNoop::new();

        let result = send_if_cond(&mut spi, &mut delay);

        spi.done();
        assert!(matches!(result, Err(Error::UnusableCard)));
    }

    #[test]
    fn send_op_cond_for_v1_supports_sdsc_as_expected() {
        let app_cmd = vec![0b0111_0111, 0, 0, 0, 0, 101];
        let op_cond_cmd = vec![0b0110_1001, 0b0000_0000, 0, 0, 0, 229];
        let expectations = [
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::write(app_cmd),
            spi::Transaction::transfer(vec![0xff], vec![0]), // R1 with no error and not idle
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::write(op_cond_cmd),
            spi::Transaction::transfer(vec![0xff], vec![0]), // R1 with no error and not idle
        ];
        let mut spi = spi::Mock::new(&expectations);
        let mut delay = delay::MockNoop::new();

        send_op_cond(&mut spi, Version::V1, &mut delay).expect("Unable to send op cond.");

        spi.done();
    }

    #[test]
    fn send_op_cond_for_v2_supports_hc_and_xc_as_expected() {
        let app_cmd = vec![0b0111_0111, 0, 0, 0, 0, 101];
        let op_cond_cmd = vec![0b0110_1001, 0b0100_0000, 0, 0, 0, 119];
        let expectations = [
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::write(app_cmd),
            spi::Transaction::transfer(vec![0xff], vec![0]), // R1 with no error and not idle
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::write(op_cond_cmd),
            spi::Transaction::transfer(vec![0xff], vec![0]), // R1 with no error and not idle
        ];
        let mut spi = spi::Mock::new(&expectations);
        let mut delay = delay::MockNoop::new();

        send_op_cond(&mut spi, Version::V2, &mut delay).expect("Unable to send op cond.");

        spi.done();
    }

    #[test]
    fn send_op_cond_with_idle_response_repeats() {
        let app_cmd = vec![0b0111_0111, 0, 0, 0, 0, 101];
        let op_cond_cmd = vec![0b0110_1001, 0b0100_0000, 0, 0, 0, 119];
        let expectations = [
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::write(app_cmd.clone()),
            spi::Transaction::transfer(vec![0xff], vec![0b0000_0001]), // R1 with no error and idle
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::write(op_cond_cmd.clone()),
            spi::Transaction::transfer(vec![0xff], vec![0b0000_0001]), // R1 with no error and idle
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::write(app_cmd),
            spi::Transaction::transfer(vec![0xff], vec![0b0000_0001]), // R1 with no error and idle
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::write(op_cond_cmd),
            spi::Transaction::transfer(vec![0xff], vec![0b0000_0000]), // R1 with no error and not idle
        ];
        let mut spi = spi::Mock::new(&expectations);
        let mut delay = delay::MockNoop::new();

        send_op_cond(&mut spi, Version::V2, &mut delay).expect("Unable to send op cond.");

        spi.done();
    }

    #[test]
    fn send_op_cond_with_repeated_idle_response_is_unuable() {
        let app_cmd = vec![0b0111_0111, 0, 0, 0, 0, 101];
        let op_cond_cmd = vec![0b0110_1001, 0b0100_0000, 0, 0, 0, 119];
        let mut expectations = Vec::new();
        for _ in 0..MAX_OP_COND_COUNT {
            expectations.extend([
                spi::Transaction::transfer(vec![0xff], vec![0xff]),
                spi::Transaction::write(app_cmd.clone()),
                spi::Transaction::transfer(vec![0xff], vec![0b0000_0001]), // R1 with no error and idle
                spi::Transaction::transfer(vec![0xff], vec![0xff]),
                spi::Transaction::write(op_cond_cmd.clone()),
                spi::Transaction::transfer(vec![0xff], vec![0b0000_0001]), // R1 with no error and idle
            ]);
        }
        let mut spi = spi::Mock::new(&expectations);
        let mut delay = delay::MockNoop::new();

        let result = send_op_cond(&mut spi, Version::V2, &mut delay);

        spi.done();
        assert_eq!(result, Err(Error::UnusableCard));
    }

    #[test]
    fn check_card_capacity_for_v1_is_standard() {
        let mut spi = spi::Mock::new(iter::empty());
        let mut delay = delay::MockNoop::new();

        let result = check_card_capacity(&mut spi, &mut delay, Version::V1);

        spi.done();
        assert_eq!(result, Ok(CardCapacity::Standard));
    }

    #[test]
    fn check_card_capacity_for_v2_without_ccs_is_standard() {
        let expectations = [
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::write(vec![0b0111_1010, 0, 0, 0, 0, 253]),
            spi::Transaction::transfer(vec![0xff], vec![0x00]),
            spi::Transaction::transfer(vec![0xff], vec![0x00]),
            spi::Transaction::transfer(vec![0xff], vec![0x00]),
            spi::Transaction::transfer(vec![0xff], vec![0x00]),
            spi::Transaction::transfer(vec![0xff], vec![0x00]),
        ];
        let mut spi = spi::Mock::new(&expectations);
        let mut delay = delay::MockNoop::new();

        let result = check_card_capacity(&mut spi, &mut delay, Version::V2);

        spi.done();
        assert_eq!(result, Ok(CardCapacity::Standard));
    }

    #[test]
    fn check_card_capacity_for_v2_withccs_is_high_or_extended() {
        let expectations = [
            spi::Transaction::transfer(vec![0xff], vec![0xff]),
            spi::Transaction::write(vec![0b0111_1010, 0, 0, 0, 0, 253]),
            spi::Transaction::transfer(vec![0xff], vec![0x00]),
            spi::Transaction::transfer(vec![0xff], vec![0b0100_0000]),
            spi::Transaction::transfer(vec![0xff], vec![0x00]),
            spi::Transaction::transfer(vec![0xff], vec![0x00]),
            spi::Transaction::transfer(vec![0xff], vec![0x00]),
        ];
        let mut spi = spi::Mock::new(&expectations);
        let mut delay = delay::MockNoop::new();

        let result = check_card_capacity(&mut spi, &mut delay, Version::V2);

        spi.done();
        assert_eq!(result, Ok(CardCapacity::HighOrExtended));
    }
}
