//! [`Device`]: one open dongle.

use crate::access::{Access, I2cAddress};
use crate::control::ControlPort;
use crate::error::Error;
use crate::portable::pending::Gap;
use crate::portable::r820t2::Registers;
use crate::portable::r820t2::registers::I2C_ADDRESS;
use crate::portable::rtl2832u::PAGE_1;
use crate::portable::sequence::{CLOSE, OPEN, Step};
use crate::portable::v5::{
    AFTER_RATIO, AFTER_RATIO_GATE, BAND_REGISTERS, FILTER_REGISTER, GAIN_REGISTERS, Gain,
    IF_REGISTER, LOCK_BIT, PLL_READS, PLL_REGISTER, R18_RETRY, RATIO_REGISTERS, RETRY_REGISTER,
    RatePlan, TunePlan,
};

/// The ledger row for every board's open but the V5's. prov: RTL-GAP-07
const OTHER_BOARD: &str = "RTL-GAP-07";

/// One RTL-SDR dongle behind a [`ControlPort`].
pub struct Device {
    /// The dongle's transport.
    port: Box<dyn ControlPort>,

    /// The sample rate's plan, once one is set.
    rate: Option<RatePlan>,

    /// The frequency tuned to, in hertz, once one is.
    frequency: Option<u32>,
}

impl Device {
    /// Brings the dongle up over `port` with the `NESDR SMArt v5`'s captured open ([`OPEN`]),
    /// the only board's open evidenced.
    ///
    /// # Errors
    ///
    /// [`Error::Gap`] (`RTL-GAP-07`) when the board is not one the open was captured on, as
    /// far as the open can tell: something answers at a probed address, or the tuner's R0
    /// is not the R820T2's chip id. [`Error::Bus`] on a transport failure, including a
    /// stall where the V5 answers (no tuner at its address, say). The dongle is left
    /// part-way either way.
    pub fn open(port: Box<dyn ControlPort>) -> Result<Self, Error> {
        let mut device = Self {
            port,
            rate: None,
            frequency: None,
        };
        device.run(OPEN)?;
        Ok(device)
    }

    /// Shuts the dongle down with the V5's captured close ([`CLOSE`]) and gives up the
    /// port. Dropping a `Device` without it sends nothing.
    ///
    /// # Errors
    ///
    /// [`Error::Bus`] on a transport failure.
    pub fn close(mut self) -> Result<(), Error> {
        self.run(CLOSE)
    }

    /// Sets the sample rate to `rate` S/s ([`RatePlan`]): the tuner's channel filter, the
    /// IF, the resampler ratio; then re-tunes, if tuned, for the new IF.
    ///
    /// # Errors
    ///
    /// Before anything is sent: [`Error::Rate`] for a rate outside
    /// [`crate::v5::RATE_RANGES`]; [`Error::Frequency`] when the frequency tuned to has
    /// no LO in range at the new IF. Once the rate is applied, the re-tune's errors as
    /// [`Device::tune`]'s. [`Error::Bus`] on a transport failure, leaving the dongle
    /// part-way (the tuner's gate possibly open).
    pub fn set_sample_rate(&mut self, rate: u32) -> Result<(), Error> {
        let plan = RatePlan::new(rate).ok_or(Error::Rate(rate))?;
        let retune = self
            .frequency
            .map(|frequency| {
                TunePlan::new(frequency, plan.if_hz)
                    .map(|tune| (frequency, tune))
                    .ok_or(Error::Frequency(frequency))
            })
            .transpose()?;
        let mut access = Access::new(&mut *self.port);
        access.set_gate(true)?;
        access.i2c_write(tuner(), &[FILTER_REGISTER, plan.filter[0]])?;
        access.i2c_write(tuner(), &[FILTER_REGISTER + 1, plan.filter[1]])?;
        access.set_gate(false)?;
        for (register, byte) in (IF_REGISTER..).zip(plan.if_bytes) {
            access.write_paged(PAGE_1, register, &[byte])?;
        }
        let [b3, b2, b1, b0] = plan.ratio.to_be_bytes();
        access.write_paged(PAGE_1, RATIO_REGISTERS[0], &[b3, b2])?;
        access.write_paged(PAGE_1, RATIO_REGISTERS[1], &[b1, b0])?;
        for register in AFTER_RATIO {
            access.write_paged(PAGE_1, register, &[0])?;
        }
        for byte in AFTER_RATIO_GATE {
            access.write_gate(byte)?;
        }
        self.rate = Some(plan);
        match retune {
            Some((frequency, tune)) => self.send_tune(frequency, tune),
            None => Ok(()),
        }
    }

    /// Tunes to `frequency` Hz ([`TunePlan`]): the band registers, the PLL, its lock.
    ///
    /// # Errors
    ///
    /// [`Error::NoRate`] before a sample rate is set; [`Error::Frequency`] when the LO is
    /// out of range, before anything is sent; [`Error::Unlocked`] when the PLL does not
    /// lock, with the gate closed; [`Error::Bus`] on a transport failure, leaving the
    /// tuner part-way (its gate possibly open). After a failure the device still counts
    /// the previous frequency as tuned, though the tuner may hold the new one in part.
    pub fn tune(&mut self, frequency: u32) -> Result<(), Error> {
        let rate = self.rate.ok_or(Error::NoRate)?;
        let plan = TunePlan::new(frequency, rate.if_hz).ok_or(Error::Frequency(frequency))?;
        self.send_tune(frequency, plan)
    }

    /// Sends `plan`, the tune to `frequency`, and records it once locked.
    fn send_tune(&mut self, frequency: u32, plan: TunePlan) -> Result<(), Error> {
        let mut access = Access::new(&mut *self.port);
        access.set_gate(true)?;
        for (register, byte) in BAND_REGISTERS.into_iter().zip(plan.bands) {
            access.i2c_write(tuner(), &[register, byte])?;
        }
        read_tuner(&mut access, &mut [0; PLL_READS[0]])?;
        let mut write = [0; 8];
        write[0] = PLL_REGISTER;
        write[1..].copy_from_slice(&plan.pll);
        access.i2c_write(tuner(), &write)?;
        let locked = locked(&mut access)? || {
            access.i2c_write(tuner(), &[RETRY_REGISTER, R18_RETRY])?;
            locked(&mut access)?
        };
        if !locked {
            access.set_gate(false)?;
            return Err(Error::Unlocked);
        }
        access.i2c_write(tuner(), &[BAND_REGISTERS[1], plan.r26_after])?;
        access.set_gate(false)?;
        self.frequency = Some(frequency);
        Ok(())
    }

    /// Sets the tuner's gain.
    ///
    /// # Errors
    ///
    /// [`Error::Bus`] on a transport failure, leaving the tuner part-way (its gate
    /// possibly open).
    pub fn set_gain(&mut self, gain: Gain) -> Result<(), Error> {
        let mut access = Access::new(&mut *self.port);
        access.set_gate(true)?;
        for (register, byte) in GAIN_REGISTERS.into_iter().zip(gain.registers()) {
            access.i2c_write(tuner(), &[register, byte])?;
        }
        access.set_gate(false)?;
        Ok(())
    }

    /// Runs `steps`, in order, stopping at the first failure.
    fn run(&mut self, steps: &[Step]) -> Result<(), Error> {
        let mut access = Access::new(&mut *self.port);
        for step in steps {
            match *step {
                Step::Paged {
                    page,
                    register,
                    data,
                } => access.write_paged(page, register, data)?,
                Step::Block {
                    block,
                    address,
                    data,
                } => access.write_block(block, address, data)?,
                Step::Gate { open } => access.set_gate(open)?,
                Step::Probe { address, byte } => {
                    let address = i2c(address);
                    let wrote = access.i2c_write(address, &[byte]).is_ok();
                    let read = access.i2c_read(address, &mut [0]).is_ok();
                    if wrote || read {
                        return Err(Gap::new(OTHER_BOARD).into());
                    }
                }
                Step::Identify { address } => {
                    let mut r0 = [0];
                    let len = access.i2c_read(i2c(address), &mut r0)?;
                    let identified = Registers::from_read(&r0[..len])
                        .expect("invariant: at most one register read")
                        .has_chip_id();
                    if !identified {
                        return Err(Gap::new(OTHER_BOARD).into());
                    }
                }
                Step::I2cWrite { address, data } => access.i2c_write(i2c(address), data)?,
                Step::I2cRead { address, len } => {
                    access.i2c_read(i2c(address), &mut vec![0; len])?;
                }
            }
        }
        Ok(())
    }
}

/// Whether the tuner's PLL reports lock: R2's lock bit, read from R0 on.
fn locked(access: &mut Access<'_>) -> Result<bool, Error> {
    let mut read = [0; PLL_READS[1]];
    let len = read_tuner(access, &mut read)?;
    let registers = Registers::from_read(&read[..len]).expect("invariant: three registers");
    Ok(registers.byte(2).is_some_and(|r2| r2 & LOCK_BIT != 0))
}

/// Reads the tuner's registers from R0 into `data`, pointing it at R0 first as every
/// captured read does (`RTL-OBS-06`); returns the bytes read.
fn read_tuner(access: &mut Access<'_>, data: &mut [u8]) -> Result<usize, Error> {
    access.i2c_write(tuner(), &[0])?;
    Ok(access.i2c_read(tuner(), data)?)
}

/// The tuner's I2C address.
fn tuner() -> I2cAddress {
    i2c(I2C_ADDRESS)
}

/// A sequence's I2C address.
fn i2c(address: u8) -> I2cAddress {
    I2cAddress::new(address).expect("invariant: the sequences' addresses are 7-bit")
}
