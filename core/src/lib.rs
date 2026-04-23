#![doc = include_str!("../README.md")]

#[cfg(not(target_os = "linux"))]
compile_error!("This crate support Linux only");

mod iop;
mod raw;
mod types;
mod utils;
mod values;

use std::{fmt, os::unix::io::RawFd};

pub use iop::RawEvent;
pub use std::{
    io::{Error, Result},
    path::{Path, PathBuf},
    time::Duration as Time,
};
pub use types::{
    Active, Bias, BitId, Direction, Drive, Edge, EdgeDetect, Event, LineId, LineInfo, LineMap,
};
pub use utils::*;
pub use values::{AsValues, AsValuesMut, Bits, Masked, Values, MAX_BITS, MAX_VALUES};

macro_rules! unsafe_call {
    ($res:expr) => {
        unsafe { $res }.map_err(Error::from)
    };
}

/// Device path trait
pub trait AsDevicePath {
    /// Get absolute device path
    fn as_device_path(&self) -> PathBuf;
}

impl AsDevicePath for Path {
    fn as_device_path(&self) -> PathBuf {
        if self.is_absolute() {
            self.to_path_buf()
        } else {
            Path::new("/dev").join(self)
        }
    }
}

impl AsDevicePath for &Path {
    fn as_device_path(&self) -> PathBuf {
        if self.is_absolute() {
            self.to_path_buf()
        } else {
            Path::new("/dev").join(self)
        }
    }
}

impl AsDevicePath for PathBuf {
    fn as_device_path(&self) -> PathBuf {
        let path: &Path = self;
        path.as_device_path()
    }
}

impl AsDevicePath for &PathBuf {
    fn as_device_path(&self) -> PathBuf {
        let path: &Path = self;
        path.as_device_path()
    }
}

impl AsDevicePath for &str {
    fn as_device_path(&self) -> PathBuf {
        Path::new(self).as_device_path()
    }
}

impl AsDevicePath for String {
    fn as_device_path(&self) -> PathBuf {
        let s: &str = self;
        s.as_device_path()
    }
}

impl AsDevicePath for &String {
    fn as_device_path(&self) -> PathBuf {
        let s: &str = self;
        s.as_device_path()
    }
}

impl AsDevicePath for usize {
    fn as_device_path(&self) -> PathBuf {
        format!("/dev/gpiochip{self}").as_device_path()
    }
}

macro_rules! as_device_path {
    ($($type:ty),*) => {
        $(
            impl AsDevicePath for $type {
                fn as_device_path(&self) -> PathBuf {
                    (*self as usize).as_device_path()
                }
            }
        )*
    };
}

as_device_path!(u8, u16, u32, u64, i8, i16, i32, i64, isize);

/// Wrapper to hide internals
#[derive(Clone, Copy, Default)]
pub struct Internal<T>(T);

impl<T> core::ops::Deref for Internal<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> core::ops::DerefMut for Internal<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// GPIO lines values interface info
pub struct ValuesInfo {
    chip_name: String,
    consumer: String,
    lines: Vec<LineId>,
    index: LineMap,
}

impl fmt::Display for ValuesInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {:?} {:?}", self.chip_name, self.consumer, self.lines)
    }
}

impl ValuesInfo {
    /// Get associated chip name
    pub fn chip_name(&self) -> &str {
        &self.chip_name
    }

    /// Get consumer string
    pub fn consumer(&self) -> &str {
        &self.consumer
    }

    /// Get offsets of requested lines
    pub fn lines(&self) -> &[LineId] {
        &self.lines
    }

    /// Get offset to bit position mapping
    pub fn index(&self) -> &LineMap {
        &self.index
    }
}

impl Internal<ValuesInfo> {
    fn new(chip_name: &str, consumer: &str, lines: &[LineId]) -> Self {
        let chip_name = chip_name.into();
        let consumer = consumer.into();
        let index = LineMap::new(lines);
        let lines = lines.to_owned();

        Self(ValuesInfo {
            chip_name,
            consumer,
            lines,
            index,
        })
    }

    pub fn get_values<T: AsValuesMut>(&self, fd: RawFd, values: &mut T) -> Result<()> {
        #[cfg(not(feature = "v2"))]
        {
            let mut data = raw::v1::GpioHandleData::default();

            unsafe_call!(raw::v1::gpio_get_line_values(fd, &mut data))?;

            data.fill_values(self.lines.len(), values);
        }

        #[cfg(feature = "v2")]
        {
            let mut data = values.convert::<Values>();
            data.truncate(self.lines.len() as _);

            unsafe_call!(raw::v2::gpio_line_get_values(fd, data.as_mut()))?;

            data.copy_into(values);
        }

        Ok(())
    }

    pub fn set_values<T: AsValues>(&self, fd: RawFd, values: T) -> Result<()> {
        #[cfg(not(feature = "v2"))]
        {
            let mut data = raw::v1::GpioHandleData::from_values(self.lines.len(), values);

            unsafe_call!(raw::v1::gpio_set_line_values(fd, &mut data))?;
        }

        #[cfg(feature = "v2")]
        {
            let mut data = values.convert::<Values>();
            data.truncate(self.lines.len() as _);

            unsafe_call!(raw::v2::gpio_line_set_values(fd, data.as_mut()))?;
        }

        Ok(())
    }
}

/// Direction trait
pub trait DirectionType: Send + Sync + 'static {
    const DIR: Direction;
}

/// Input direction
pub struct Input;

impl DirectionType for Input {
    const DIR: Direction = Direction::Input;
}

/// Output direction
pub struct Output;

impl DirectionType for Output {
    const DIR: Direction = Direction::Output;
}

/// GPIO line values request options
///
/// Input config:
/// ```
/// # use gpiod_core::{Options, Active, Bias};
/// let input = Options::input(&[23, 17, 3])
///     .active(Active::Low)
///     .bias(Bias::PullUp)
///     .consumer("my inputs");
/// ```
///
/// Output config:
/// ```
/// # use gpiod_core::{Options, Active, Drive};
/// let output = Options::output(&[11, 20])
///     .active(Active::Low)
///     .drive(Drive::PushPull)
///     .values([false, true])
///     .consumer("my outputs");
/// ```
///
/// Input with edge detection:
/// ```
/// # use gpiod_core::{Options, Active, Bias, EdgeDetect};
/// let input = Options::input(&[21, 13])
///     .active(Active::Low)
///     .bias(Bias::PullUp)
///     .edge(EdgeDetect::Both)
///     .consumer("my inputs");
/// ```
pub struct Options<Direction = (), Lines = (), Consumer = ()> {
    lines: Lines,
    direction: core::marker::PhantomData<Direction>,
    active: Active,
    edge: Option<EdgeDetect>,
    bias: Option<Bias>,
    debounce_period: Option<Time>,
    drive: Option<Drive>,
    values: Option<Values>,
    consumer: Consumer,
}

impl Options {
    /// Create input options
    pub fn input<Lines: AsRef<[LineId]>>(lines: Lines) -> Options<Input, Lines, &'static str> {
        Options::<Input, Lines, &'static str> {
            lines,
            direction: Default::default(),
            active: Default::default(),
            edge: Default::default(),
            bias: Default::default(),
            debounce_period: Default::default(),
            drive: Default::default(),
            values: Default::default(),
            consumer: "",
        }
    }

    /// Create output options
    pub fn output<Lines: AsRef<[LineId]>>(lines: Lines) -> Options<Output, Lines, &'static str> {
        Options::<Output, Lines, &'static str> {
            lines,
            direction: Default::default(),
            active: Default::default(),
            edge: Default::default(),
            bias: Default::default(),
            debounce_period: Default::default(),
            drive: Default::default(),
            values: Default::default(),
            consumer: "",
        }
    }
}

impl<Direction, Lines, OldConsumer> Options<Direction, Lines, OldConsumer> {
    /// Configure consumer string
    pub fn consumer<Consumer: AsRef<str>>(
        self,
        consumer: Consumer,
    ) -> Options<Direction, Lines, Consumer> {
        Options::<Direction, Lines, Consumer> {
            lines: self.lines,
            direction: self.direction,
            active: self.active,
            edge: self.edge,
            bias: self.bias,
            debounce_period: self.debounce_period,
            drive: self.drive,
            values: self.values,
            consumer,
        }
    }
}

impl<Direction, Lines, Consumer> Options<Direction, Lines, Consumer> {
    /// Configure GPIO lines astive state
    ///
    /// Available both for inputs and outputs
    pub fn active(mut self, active: Active) -> Self {
        self.active = active;
        self
    }

    /// Configure GPIO lines bias
    ///
    /// Available both for inputs and outputs
    pub fn bias(mut self, bias: Bias) -> Self {
        self.bias = Some(bias);
        self
    }
}

impl<Direction, Lines: AsRef<[LineId]>, Consumer: AsRef<str>> Options<Direction, Lines, Consumer> {
    /// Make an independent copy of options
    pub fn to_owned(&self) -> Options<Direction, Vec<LineId>, String> {
        Options::<Direction, Vec<LineId>, String> {
            lines: self.lines.as_ref().to_owned(),
            direction: self.direction,
            active: self.active,
            edge: self.edge,
            bias: self.bias,
            debounce_period: self.debounce_period,
            drive: self.drive,
            values: self.values,
            consumer: self.consumer.as_ref().to_owned(),
        }
    }
}

impl<Lines, Consumer> Options<Input, Lines, Consumer> {
    /// Configure edge detection
    ///
    /// Available only for inputs
    pub fn edge(mut self, edge: EdgeDetect) -> Self {
        self.edge = Some(edge);
        self
    }

    /// Configure GPIO input debounce period.
    ///
    /// Available only for inputs.
    ///
    /// This is supported only by the Linux GPIO character device v2 ABI.
    /// It is ignored when the crate is built without the `v2` feature.
    pub fn debounce(mut self, debounce_period: Time) -> Self {
        self.debounce_period = Some(debounce_period);
        self
    }
}

impl<Lines, Consumer> Options<Output, Lines, Consumer> {
    /// Configure edge detection
    ///
    /// Available only for outputs
    pub fn drive(mut self, drive: Drive) -> Self {
        self.drive = Some(drive);
        self
    }

    /// Configure default values
    ///
    /// Available only for outputs
    pub fn values<T: AsValues>(mut self, values: T) -> Self {
        self.values = Some(values.convert());
        self
    }
}

/// GPIO chip interface info
pub struct ChipInfo {
    name: String,
    label: String,
    num_lines: LineId,
}

impl fmt::Display for ChipInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} [{}] ({} lines)",
            self.name, self.label, self.num_lines
        )
    }
}

impl ChipInfo {
    /// Get chip name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get chip label
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Get number of GPIO lines
    pub fn num_lines(&self) -> LineId {
        self.num_lines
    }
}

impl Internal<ChipInfo> {
    pub fn from_fd(fd: RawFd) -> Result<Self> {
        let mut info = raw::GpioChipInfo::default();

        unsafe_call!(raw::gpio_get_chip_info(fd, &mut info))?;

        Ok(Self(ChipInfo {
            name: safe_get_str(&info.name)?.into(),
            label: safe_get_str(&info.label)?.into(),
            num_lines: info.lines,
        }))
    }

    /// Request the info of a specific GPIO line.
    pub fn line_info(&self, fd: RawFd, line: LineId) -> Result<LineInfo> {
        #[cfg(not(feature = "v2"))]
        {
            let mut info = raw::v1::GpioLineInfo {
                line_offset: line,
                ..Default::default()
            };

            unsafe_call!(raw::v1::gpio_get_line_info(fd, &mut info))?;

            info.as_info()
        }

        #[cfg(feature = "v2")]
        {
            let mut info = raw::v2::GpioLineInfo::default();

            info.offset = line;

            unsafe_call!(raw::v2::gpio_get_line_info(fd, &mut info))?;

            info.as_info()
        }
    }

    /// Request the GPIO chip to configure the lines passed as argument as outputs
    ///
    /// Calling this operation is a precondition to being able to set the state of the GPIO lines.
    /// All the lines passed in one request must share the output mode and the active state.
    pub fn request_lines<Direction: DirectionType>(
        &self,
        fd: RawFd,
        options: Options<Direction, impl AsRef<[LineId]>, impl AsRef<str>>,
    ) -> Result<(Internal<ValuesInfo>, RawFd)> {
        let Options {
            lines,
            direction: _,
            active,
            edge,
            bias,
            debounce_period,
            drive,
            values,
            consumer,
        } = options;

        let direction = Direction::DIR;
        let lines = lines.as_ref();
        let consumer = consumer.as_ref();

        #[cfg(not(feature = "v2"))]
        let fd = {
            let _ = debounce_period;
            let mut request =
                raw::v1::GpioHandleRequest::new(lines, direction, active, bias, drive, consumer)?;

            // TODO: edge detection

            unsafe_call!(raw::v1::gpio_get_line_handle(fd, &mut request))?;

            if let Some(values) = values {
                let mut data = raw::v1::GpioHandleData::from_values(lines.len(), &values);

                unsafe_call!(raw::v1::gpio_set_line_values(fd, &mut data))?;
            }

            request.fd
        };

        #[cfg(feature = "v2")]
        let fd = {
            let mut request = raw::v2::GpioLineRequest::new(
                lines,
                direction,
                active,
                edge,
                bias,
                debounce_period,
                drive,
                values,
                consumer,
            )?;

            unsafe_call!(raw::v2::gpio_get_line(fd, &mut request))?;

            request.fd
        };

        Ok((Internal::<ValuesInfo>::new(&self.name, consumer, lines), fd))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn input_options() {
        let _ = Options::input([27, 1, 19])
            .bias(Bias::PullUp)
            .active(Active::Low)
            .edge(EdgeDetect::Both)
            .consumer("gpin");
    }

    #[test]
    fn output_options() {
        let _ = Options::output([11, 2])
            .bias(Bias::PullUp)
            .active(Active::Low)
            .consumer("gpout")
            .drive(Drive::OpenDrain)
            .values([true, false]);
    }
}
