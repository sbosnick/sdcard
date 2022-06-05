# SDCard

**An emdedded-hal driver for SDCards.**


---

SDCard is an `emdedded-hal` driver for SDCards that exposes them through the
`emdedded-storage` traits. This driver is designed to access an SDCard through
the SPI interface defined in Part 1 of the [Simplifed Specification].

The core SDCard crate is a platform independent driver which is tested with
the `emdedded-hal-mock` crate.

[Simplifed Specification]: https://www.sdcard.org/downloads/pls/

## License

SDCard is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE-2.0](LICENSE-APACHE-2.0) or
   http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   http://opensource.org/licenses/MIT)

at your option.

## Contribution

Currently the best way to contribute to SDCard is through filing issues on
GitHub. If you are also able to provide a pull request for need changes that
would be approciated.

Please note that this project is released with a [Contributor Code of
Conduct][code-of-conduct].  By participating in this project you agree to abide
by its terms.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Calandon by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

[code-of-conduct]: CODE_OF_CONDUCT.md
