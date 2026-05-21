#![allow(dead_code)]
#![allow(clippy::upper_case_acronyms)]

use esp_hal::{Blocking, delay::Delay, i2c::master::I2c};

#[derive(Copy, Clone, Debug)]
pub struct LcdError;

#[derive(Copy, Clone, Debug)]
pub enum Cursor {
    On = 0x02,
    Off = 0x00,
}

#[derive(Copy, Clone, Debug)]
pub enum Blink {
    On = 0x01,
    Off = 0x00,
}

#[derive(Copy, Clone, Debug)]
pub enum Display {
    On = 0x04,
    Off = 0x00,
}

#[derive(Copy, Clone, Debug)]
pub enum Backlight {
    On = 0x08,
    Off = 0x00,
}

#[derive(Copy, Clone, Debug)]
pub enum Mode {
    COMMAND = 0x00,
    CLEARDISPLAY = 0x01,
    RETURNHOME = 0x02,
    ENTRYMODESET = 0x04,
    DISPLAYCONTROL = 0x08,
    CURSORSHIFT = 0x10,
    FUNCTIONSET = 0x20,
    SETCGRAMADDR = 0x40,
    SETDDRAMADDR = 0x80,
}

#[derive(Copy, Clone, Debug)]
pub enum Entries {
    RIGHT = 0x00,
    LEFT = 0x02,
}

#[derive(Copy, Clone, Debug)]
pub enum MoveSelect {
    DISPLAY = 0x08,
    CURSOR = 0x00,
}

#[derive(Copy, Clone, Debug)]
pub enum Direction {
    RIGHT = 0x04,
    LEFT = 0x00,
}

#[derive(Copy, Clone, Debug)]
pub enum Shift {
    INCREMENT = 0x01,
    DECREMENT = 0x00,
}

#[derive(Copy, Clone, Debug)]
pub enum BitMode {
    Bit4 = 0x00,
    Bit8 = 0x10,
}

#[derive(Copy, Clone, Debug)]
pub enum Dots {
    Dots5x8 = 0x00,
    Dots5x10 = 0x04,
}

#[derive(Copy, Clone, Debug)]
pub enum Lines {
    OneLine = 0x00,
    TwoLine = 0x08,
}

#[derive(Copy, Clone, Debug)]
pub enum BitAction {
    Command = 0x00,
    Enable = 0x04,
    ReadWrite = 0x02,
    RegisterSelect = 0x01,
}

pub struct DisplayControl {
    pub cursor: Cursor,
    pub display: Display,
    pub blink: Blink,
    pub backlight: Backlight,
    pub direction: Direction,
}

impl DisplayControl {
    pub fn new() -> Self {
        DisplayControl {
            cursor: Cursor::Off,
            display: Display::Off,
            blink: Blink::Off,
            backlight: Backlight::On,
            direction: Direction::LEFT,
        }
    }

    pub fn value(&self) -> u8 {
        self.blink as u8 | self.cursor as u8 | self.display as u8 | self.backlight as u8
    }
}

impl Default for DisplayControl {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Lcd<'a> {
    i2c: I2c<'a, Blocking>,
    control: DisplayControl,
    address: u8,
    delay: Delay,
}

impl<'a> Lcd<'a> {
    pub fn new(i2c: I2c<'a, Blocking>, address: u8) -> Result<Self, LcdError> {
        let mut display = Self {
            i2c,
            control: DisplayControl::new(),
            address,
            delay: Delay::new(),
        };

        display.init()?;
        Ok(display)
    }

    fn init(&mut self) -> Result<(), LcdError> {
        self.delay.delay_millis(50);

        self.i2c_write(self.control.backlight as u8)?;
        self.delay.delay_millis(1);

        let mode_8bit = Mode::FUNCTIONSET as u8 | BitMode::Bit8 as u8;
        self.write_nibble(mode_8bit)?;
        self.delay.delay_millis(5);

        self.write_nibble(mode_8bit)?;
        self.delay.delay_millis(5);

        self.write_nibble(mode_8bit)?;
        self.delay.delay_millis(5);

        let mode_4bit = Mode::FUNCTIONSET as u8 | BitMode::Bit4 as u8;
        self.write_nibble(mode_4bit)?;
        self.delay.delay_millis(5);

        let lines_font = Mode::FUNCTIONSET as u8
            | BitMode::Bit4 as u8
            | Dots::Dots5x8 as u8
            | Lines::TwoLine as u8;
        self.command(lines_font)?;

        self.clear()?;

        let entry_mode = Mode::ENTRYMODESET as u8 | Entries::LEFT as u8 | Shift::DECREMENT as u8;
        self.command(entry_mode)?;

        Ok(())
    }

    pub fn clear(&mut self) -> Result<(), LcdError> {
        self.command(Mode::CLEARDISPLAY as u8)?;
        self.delay.delay_millis(2);
        Ok(())
    }

    pub fn home(&mut self) -> Result<(), LcdError> {
        self.command(Mode::RETURNHOME as u8)?;
        self.delay.delay_millis(2);
        Ok(())
    }

    pub fn set_cursor_position(&mut self, col: u8, row: u8) -> Result<(), LcdError> {
        self.command(Mode::SETDDRAMADDR as u8 | (col + row * 0x40))?;
        Ok(())
    }

    pub fn set_display(&mut self, display: Display) -> Result<(), LcdError> {
        self.control.display = display;
        self.write_display_control()
    }

    pub fn set_cursor(&mut self, cursor: Cursor) -> Result<(), LcdError> {
        self.control.cursor = cursor;
        self.write_display_control()
    }

    pub fn set_blink(&mut self, blink: Blink) -> Result<(), LcdError> {
        self.control.blink = blink;
        self.write_display_control()
    }

    pub fn set_backlight(&mut self, backlight: Backlight) -> Result<(), LcdError> {
        self.control.backlight = backlight;
        self.i2c_write(0)
    }

    pub fn print(&mut self, s: &str) -> Result<(), LcdError> {
        for c in s.chars() {
            self.write(c as u8)?;
        }

        Ok(())
    }

    fn write_display_control(&mut self) -> Result<(), LcdError> {
        self.command(Mode::DISPLAYCONTROL as u8 | self.control.value())
    }

    fn write(&mut self, value: u8) -> Result<(), LcdError> {
        self.send(value, BitAction::RegisterSelect)
    }

    fn command(&mut self, value: u8) -> Result<(), LcdError> {
        self.send(value, BitAction::Command)
    }

    fn send(&mut self, data: u8, mode: BitAction) -> Result<(), LcdError> {
        let high_bits: u8 = data & 0xf0;
        let low_bits: u8 = (data << 4) & 0xf0;
        self.write_nibble(high_bits | mode as u8)?;
        self.write_nibble(low_bits | mode as u8)?;
        Ok(())
    }

    fn write_nibble(&mut self, value: u8) -> Result<(), LcdError> {
        self.i2c_write(value)?;
        self.pulse_enable(value)?;
        Ok(())
    }

    fn i2c_write(&mut self, data: u8) -> Result<(), LcdError> {
        self.i2c
            .write(self.address, &[data | self.control.backlight as u8])
            .unwrap();
        Ok(())
    }

    fn pulse_enable(&mut self, data: u8) -> Result<(), LcdError> {
        self.i2c_write(data | BitAction::Enable as u8)?;
        self.delay.delay_micros(1);

        self.i2c_write(data & !(BitAction::Enable as u8))?;
        self.delay.delay_micros(1);

        Ok(())
    }
}
