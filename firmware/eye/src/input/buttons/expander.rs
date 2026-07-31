use bao1x_api::iox::IoxHal;
use bao1x_api::{I2cApi, I2cResult, IoxEnable, IoxFunction, IoxValue};
use bao1x_hal::i2c::I2c;

use super::Inputs;
use crate::pins;

// Every input on the input board is active-low: each button and each switch throw connects
// its expander pin to GND, and the MCP23008's internal pull-ups hold the rest HIGH. The
// switch's center position connects neither throw, so both switch lines read HIGH.
//
// The expander asserts INT whenever one of those pins changes, which is what keeps this off
// the I2C bus while nobody is touching the panel - the common case by a wide margin.

// MCP23008 register map.
const IODIR:   u8 = 0x00; // 1 = pin is an input
const GPINTEN: u8 = 0x02; // 1 = pin raises INT when it changes
const INTCON:  u8 = 0x04; // 0 = compare against the previous value rather than a fixed one
const IOCON:   u8 = 0x05;
const GPPU:    u8 = 0x06; // 1 = internal pull-up on
const GPIO:    u8 = 0x09; // reading this also clears a pending interrupt

const IOCON_ODR: u8 = 0x04; // INT open-drain, so the pull-up at the eye sets the idle level

/// Interrupt-on-change enables. Bit 7 is spare and stays masked so an unconnected pin
/// cannot generate interrupts.
const USED_BITS: u8 = 0x7F;

/// Read the expander at least this often even without an interrupt, so a panel that was
/// unplugged, swapped, or that dropped an edge still converges on the right state.
const BACKSTOP_TICKS: u32 = (250 / super::POLL_MS) as u32;

/// Panel inputs read over I2C from a separate input board.
pub struct Source {
    iox:        IoxHal,
    i2c:        I2c,
    configured: bool,
    ticks:      u32,
}

impl Source {
    pub fn new() -> Self {
        let iox = IoxHal::new();
        // The expander drives INT open-drain, so the eye supplies the pull-up.
        pins::setup_input_pin(
            &iox,
            pins::EXPANDER_INT_PORT,
            pins::EXPANDER_INT_PIN,
            IoxFunction::Gpio,
            IoxEnable::Enable,
        );
        let mut source = Source { iox, i2c: I2c::new(), configured: false, ticks: 0 };
        source.configured = source.configure();
        if !source.configured {
            log::warn!("input board did not answer at boot; retrying while it stays absent");
        }
        source
    }

    /// Returns a reading only when the expander reports a change or the backstop falls due.
    /// `None` means nothing new, and the caller keeps the state it already had.
    pub fn read(&mut self) -> Option<Inputs> {
        self.ticks = self.ticks.saturating_add(1);
        if self.ticks < BACKSTOP_TICKS && !self.int_asserted() {
            return None;
        }
        self.ticks = 0;

        if !self.configured {
            self.configured = self.configure();
            if !self.configured {
                return None;
            }
        }

        match self.read_port() {
            Some(bits) => Some(decode(bits)),
            None => {
                // A panel that lost power comes back at the expander's reset defaults, so
                // reconfigure before trusting another reading.
                self.configured = false;
                None
            }
        }
    }

    fn int_asserted(&self) -> bool {
        self.iox.get_gpio_pin_value(pins::EXPANDER_INT_PORT, pins::EXPANDER_INT_PIN) == IoxValue::Low
    }

    /// Put the expander into the state this driver expects. Returns false if the input board
    /// did not answer, leaving the caller to retry on the next backstop tick.
    fn configure(&mut self) -> bool {
        self.write_reg(IODIR, 0xFF)             // every pin an input
            && self.write_reg(GPPU, 0xFF)       // internal pull-ups on, so the board needs none
            && self.write_reg(INTCON, 0x00)     // interrupt on change
            && self.write_reg(IOCON, IOCON_ODR)
            && self.write_reg(GPINTEN, USED_BITS)
    }

    fn write_reg(&mut self, reg: u8, value: u8) -> bool {
        matches!(self.i2c.i2c_write(pins::EXPANDER_ADDR, reg, &[value]), Ok(I2cResult::Ack(_)))
    }

    fn read_port(&mut self) -> Option<u8> {
        let mut buf = [0u8; 1];
        match self.i2c.i2c_read(pins::EXPANDER_ADDR, GPIO, &mut buf, false) {
            Ok(I2cResult::Ack(_)) => Some(buf[0]),
            _ => None,
        }
    }
}

/// A bit reads LOW while its button is held or its switch throw is selected.
fn decode(bits: u8) -> Inputs {
    let selected = |bit: u8| bits & (1 << bit) == 0;
    Inputs {
        up:     selected(pins::EXP_BIT_UP),
        down:   selected(pins::EXP_BIT_DOWN),
        left:   selected(pins::EXP_BIT_LEFT),
        right:  selected(pins::EXP_BIT_RIGHT),
        center: selected(pins::EXP_BIT_CENTER),
        sw_on:  selected(pins::EXP_BIT_SW_ON),
        sw_off: selected(pins::EXP_BIT_SW_OFF),
    }
}
