//! # Analog to Digital converter
use crate::gpio::*;
use crate::rcc::Rcc;
use crate::stm32::ADC;
use hal::adc::{Channel, OneShot};

/// ADC Result Alignment
#[derive(PartialEq)]
pub enum Align {
    /// Right aligned results (least significant bits)
    ///
    /// Results in all precisions returning values from 0-(2^bits-1) in
    /// steps of 1.
    Right,
    /// Left aligned results (most significant bits)
    ///
    /// Results in all precisions returning a value in the range 0-65535.
    /// Depending on the precision the result will step by larger or smaller
    /// amounts.
    Left,
}

/// ADC Sampling Precision
#[derive(Copy, Clone, PartialEq)]
pub enum Precision {
    /// 12 bit precision
    B_12 = 0b00,
    /// 10 bit precision
    B_10 = 0b01,
    /// 8 bit precision
    B_8 = 0b10,
    /// 6 bit precision
    B_6 = 0b11,
}

/// ADC Sampling time
#[derive(Copy, Clone, PartialEq)]
pub enum SampleTime {
    T_2 = 0b000,
    T_4 = 0b001,
    T_8 = 0b010,
    T_12 = 0b011,
    T_20 = 0b100,
    T_40 = 0b101,
    T_80 = 0b110,
    T_160 = 0b111,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClockSource {
    Pclk(PclkDiv),
    Async(AsyncClockDiv),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PclkDiv {
    PclkD1 = 3,
    PclkD2 = 1,
    PclkD4 = 2,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AsyncClockDiv {
    AsyncD1 = 0,
    AsyncD2 = 1,
    AsyncD4 = 2,
    AsyncD8 = 3,
    AsyncD16 = 4,
    AsyncD32 = 5,
    AsyncD64 = 6,
    AsyncD128 = 7,
    AsyncD256 = 8,
}

/// Analog to Digital converter interface
pub struct Adc {
    rb: ADC,
    sample_time: SampleTime,
    align: Align,
    precision: Precision,
}

/// Contains the calibration factors for the ADC which can be reused with [`Adc::set_calibration()`]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CalibrationFactor(pub u8);

impl Adc {
    pub fn new(adc: ADC, rcc: &mut Rcc) -> Self {
        // Enable ADC clocks
        rcc.rb.apbenr2.modify(|_, w| w.adcen().set_bit());
        adc.cr.modify(|_, w| w.advregen().set_bit());

        Self {
            rb: adc,
            sample_time: SampleTime::T_2,
            align: Align::Right,
            precision: Precision::B_12,
        }
    }

    pub fn set_clock_source(&mut self, clock_source: ClockSource) {
        match clock_source {
            ClockSource::Pclk(div) => self
                .rb
                .cfgr2
                .modify(|_, w| unsafe { w.ckmode().bits(div as u8) }),
            ClockSource::Async(div) => {
                self.rb.cfgr2.modify(|_, w| unsafe { w.ckmode().bits(0) });
                self.rb
                    .ccr
                    .modify(|_, w| unsafe { w.presc().bits(div as u8) });
            }
        }
    }

    /// Runs the calibration routine on the ADC
    ///
    /// Wait for tADCVREG_SETUP (20us on STM32G071x8) after calling [`Self::new()`] before calibrating, to wait for the
    /// ADC voltage regulator to stabilize.
    ///
    /// Do not call if an ADC reading is ongoing.
    pub fn calibrate(&mut self) {
        self.rb.cr.modify(|_, w| w.adcal().set_bit());
        while self.rb.cr.read().adcal().bit_is_set() {}
    }

    /// Returns the calibration factors used by the ADC
    ///
    /// The ADC does not have a factory-stored calibration, [`Self::calibrate()`] must be run before calling this
    /// for the returned value to be useful.
    ///
    /// The ADC loses its calibration factors when Standby or Vbat mode is entered. Saving and restoring the calibration
    /// factors can be used to recalibrate the ADC after waking up from sleep more quickly than re-running calibraiton.
    /// Note that VDDA changes and to a lesser extent temperature changes affect the ADC operating conditions and
    /// calibration should be run again for the best accuracy.
    pub fn get_calibration(&self) -> CalibrationFactor {
        CalibrationFactor(self.rb.calfact.read().calfact().bits())
    }

    /// Writes the calibration factors used by the ADC
    ///
    /// See [`Self::get_calibration()`].
    ///
    /// Do not call if an ADC reading is ongoing.
    pub fn set_calibration(&mut self, calfact: CalibrationFactor) {
        self.rb
            .calfact
            .write(|w| unsafe { w.calfact().bits(calfact.0) });
    }

    /// Set the Adc sampling time
    pub fn set_sample_time(&mut self, t_samp: SampleTime) {
        self.sample_time = t_samp;
    }

    /// Set the Adc result alignment
    pub fn set_align(&mut self, align: Align) {
        self.align = align;
    }

    /// Set the Adc precision
    pub fn set_precision(&mut self, precision: Precision) {
        self.precision = precision;
    }

    fn power_up(&mut self) {
        self.rb.isr.modify(|_, w| w.adrdy().set_bit());
        self.rb.cr.modify(|_, w| w.aden().set_bit());
        while self.rb.isr.read().adrdy().bit_is_clear() {}
    }

    fn power_down(&mut self) {
        self.rb.cr.modify(|_, w| w.addis().set_bit());
        self.rb.isr.modify(|_, w| w.adrdy().set_bit());
        while self.rb.cr.read().aden().bit_is_set() {}
    }

    pub fn release(self) -> ADC {
        self.rb
    }
}

pub trait AdcExt {
    fn constrain(self, rcc: &mut Rcc) -> Adc;
}

impl AdcExt for ADC {
    fn constrain(self, rcc: &mut Rcc) -> Adc {
        Adc::new(self, rcc)
    }
}

impl<WORD, PIN> OneShot<Adc, WORD, PIN> for Adc
where
    WORD: From<u16>,
    PIN: Channel<Adc, ID = u8>,
{
    type Error = ();

    fn read(&mut self, _pin: &mut PIN) -> nb::Result<WORD, Self::Error> {
        self.power_up();
        self.rb.cfgr1.modify(|_, w| unsafe {
            w.res()
                .bits(self.precision as u8)
                .align()
                .bit(self.align == Align::Left)
        });

        self.rb
            .smpr
            .modify(|_, w| unsafe { w.smp1().bits(self.sample_time as u8) });

        self.rb
            .chselr()
            .modify(|_, w| unsafe { w.chsel().bits(1 << PIN::channel()) });

        self.rb.isr.modify(|_, w| w.eos().set_bit());
        self.rb.cr.modify(|_, w| w.adstart().set_bit());
        while self.rb.isr.read().eos().bit_is_clear() {}

        let res = self.rb.dr.read().bits() as u16;
        let val = if self.align == Align::Left && self.precision == Precision::B_6 {
            res << 8
        } else {
            res
        };

        self.power_down();
        Ok(val.into())
    }
}

macro_rules! int_adc {
    ($($Chan:ident: ($chan:expr, $en:ident)),+ $(,)*) => {
        $(
            pub struct $Chan;

            impl $Chan {
                pub fn new() -> Self {
                    Self {}
                }

                pub fn enable(&mut self, adc: &mut Adc) {
                    adc.rb.ccr.modify(|_, w| w.$en().set_bit());
                }

                pub fn disable(&mut self, adc: &mut Adc) {
                    adc.rb.ccr.modify(|_, w| w.$en().clear_bit());
                }
            }

            impl Default for $Chan {
                fn default() -> $Chan {
                    $Chan::new()
                }
            }

            impl Channel<Adc> for $Chan {
                type ID = u8;

                fn channel() -> u8 {
                    $chan
                }
            }
        )+
    };
}

macro_rules! adc_pin {
    ($($Chan:ty: ($pin:ty, $chan:expr)),+ $(,)*) => {
        $(
            impl Channel<Adc> for $pin {
                type ID = u8;

                fn channel() -> u8 { $chan }
            }
        )+
    };
}

int_adc! {
    VTemp: (12, tsen),
    VRef: (13, vrefen),
    VBat: (14, vbaten),
}

adc_pin! {
    Channel0: (gpioa::PA0<Analog>, 0u8),
    Channel1: (gpioa::PA1<Analog>, 1u8),
    Channel2: (gpioa::PA2<Analog>, 2u8),
    Channel3: (gpioa::PA3<Analog>, 3u8),
    Channel4: (gpioa::PA4<Analog>, 4u8),
    Channel5: (gpioa::PA5<Analog>, 5u8),
    Channel6: (gpioa::PA6<Analog>, 6u8),
    Channel7: (gpioa::PA7<Analog>, 7u8),
    Channel8: (gpiob::PB0<Analog>, 8u8),
    Channel9: (gpiob::PB1<Analog>, 9u8),
    Channel10: (gpiob::PB2<Analog>, 10u8),
    Channel11: (gpiob::PB10<Analog>, 11u8),
    Channel15: (gpiob::PB11<Analog>, 15u8),
    Channel16: (gpiob::PB12<Analog>, 16u8),
    Channel17: (gpioc::PC4<Analog>, 17u8),
    Channel18: (gpioc::PC5<Analog>, 18u8),
}
