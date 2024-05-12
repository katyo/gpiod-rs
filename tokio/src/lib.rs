#![doc = include_str!("../README.md")]

use std::{
    fs::File,
    io::{ErrorKind, Read},
    marker::PhantomData,
    ops::Deref,
    os::unix::{
        fs::{FileTypeExt, MetadataExt},
        io::{AsRawFd, FromRawFd},
    },
    path::{Path, PathBuf},
    sync::Arc,
};

use gpiod_core::{
    invalid_input, major, minor, set_nonblock, AsDevicePath, Error, Internal, Result,
};

pub use gpiod_core::{
    Active, AsValues, AsValuesMut, Bias, BitId, ChipInfo, Direction, DirectionType, Drive, Edge,
    EdgeDetect, Event, Input, LineId, LineInfo, Masked, Options, Output, Values, ValuesInfo,
    MAX_BITS, MAX_VALUES,
};

use tokio::{fs, fs::OpenOptions, io::unix::AsyncFd, task::spawn_blocking};

async fn asyncify<F, T>(f: F) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    match spawn_blocking(f).await {
        Ok(res) => res,
        Err(_) => Err(Error::new(ErrorKind::Other, "background task failed")),
    }
}

/// The interface for getting the values of GPIO lines configured for input
///
/// Use [Chip::request_lines] with [Options::input] or [Options::output] to configure specific
/// GPIO lines for input or output.
pub struct Lines<Direction> {
    dir: PhantomData<Direction>,
    info: Arc<Internal<ValuesInfo>>,
    // wrap file to call close on drop
    file: AsyncFd<File>,
}

impl Deref for Lines<Input> {
    type Target = ValuesInfo;

    fn deref(&self) -> &Self::Target {
        &self.info
    }
}

impl Deref for Lines<Output> {
    type Target = ValuesInfo;

    fn deref(&self) -> &Self::Target {
        &self.info
    }
}

impl<Direction: DirectionType> Lines<Direction> {
    /// Get the value of GPIO lines
    ///
    /// The values can only be read if the lines have previously been requested as inputs
    /// or outputs using the [Chip::request_lines] method with [Options::input] or with
    /// [Options::output].
    pub async fn get_values<T: AsValuesMut + Send + 'static>(&self, mut values: T) -> Result<T> {
        let fd = self.file.as_raw_fd();
        let info = self.info.clone();
        asyncify(move || info.get_values(fd, &mut values).map(|_| values)).await
    }
}

impl Lines<Input> {
    /// Read GPIO events
    ///
    /// The values can only be read if the lines have previously been requested as inputs
    /// using the [Chip::request_lines] method with [Options::input].
    pub async fn read_event(&mut self) -> Result<Event> {
        #[cfg(not(feature = "v2"))]
        {
            todo!();
        }

        #[cfg(feature = "v2")]
        {
            let _ = self.file.readable().await?;

            let mut event = gpiod_core::RawEvent::default();
            let len = self.file.get_ref().read(event.as_mut())?;

            gpiod_core::check_size(len, &event)?;

            event.as_event(self.info.index())
        }
    }
}

impl Lines<Output> {
    /// Set the value of GPIO lines
    ///
    /// The value can only be set if the lines have previously been requested as outputs
    /// using the [Chip::request_lines] with [Options::output].
    pub async fn set_values<T: AsValues + Send + 'static>(&self, values: T) -> Result<()> {
        let fd = self.file.as_raw_fd();
        let info = self.info.clone();
        asyncify(move || info.set_values(fd, values)).await
    }
}

/// A Linux chardev GPIO chip interface
///
/// It can be used to get information about the chip and lines and
/// to request GPIO lines that can be used as inputs or outputs.
pub struct Chip {
    info: Arc<Internal<ChipInfo>>,
    // wrap file to call close on drop
    file: AsyncFd<File>,
}

impl Deref for Chip {
    type Target = ChipInfo;

    fn deref(&self) -> &Self::Target {
        &self.info
    }
}

impl core::fmt::Display for Chip {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.info.fmt(f)
    }
}

impl Chip {
    /// Create a new GPIO chip interface using path
    pub async fn new(device: impl AsDevicePath) -> Result<Chip> {
        let path = device.as_device_path();

        Chip::check_device(&path).await?;

        let file = AsyncFd::new(
            OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(path)
                .await?
                .into_std()
                .await,
        )?;

        let fd = file.get_ref().as_raw_fd();
        let info = Arc::new(asyncify(move || Internal::<ChipInfo>::from_fd(fd)).await?);

        Ok(Chip { info, file })
    }

    /// List all found chips
    pub async fn list_devices() -> Result<Vec<PathBuf>> {
        let mut devices = Vec::new();
        let mut dir = fs::read_dir("/dev").await?;

        while let Some(ent) = dir.next_entry().await? {
            let path = ent.path();
            if Self::check_device(&path).await.is_ok() {
                devices.push(path);
            }
        }

        Ok(devices)
    }

    async fn check_device(path: &Path) -> Result<()> {
        let metadata = fs::symlink_metadata(&path).await?;

        /* Is it a character device? */
        if !metadata.file_type().is_char_device() {
            return Err(invalid_input("File is not character device"));
        }

        let rdev = metadata.rdev();

        /* Is the device associated with the GPIO subsystem? */
        if fs::canonicalize(format!(
            "/sys/dev/char/{}:{}/subsystem",
            major(rdev),
            minor(rdev)
        ))
        .await?
            != Path::new("/sys/bus/gpio")
        {
            return Err(invalid_input("Character device is not a GPIO"));
        }

        Ok(())
    }

    /// Request the info of a specific GPIO line.
    pub async fn line_info(&self, line: LineId) -> Result<LineInfo> {
        let fd = self.file.as_raw_fd();
        let info = self.info.clone();
        asyncify(move || info.line_info(fd, line)).await
    }

    /// Request the GPIO chip to configure the lines passed as argument as inputs or outputs
    ///
    /// Calling this operation is a precondition to being able to set the state of the GPIO lines.
    /// All the lines passed in one request must share the configured options such as active state,
    /// edge detect, GPIO bias, output drive and consumer string.
    pub async fn request_lines<Direction: DirectionType>(
        &self,
        options: Options<Direction, impl AsRef<[LineId]>, impl AsRef<str>>,
    ) -> Result<Lines<Direction>> {
        let fd = self.file.as_raw_fd();
        let options = options.to_owned();
        let info = self.info.clone();

        let (info, fd) = asyncify(move || {
            let (info, fd) = info.request_lines(fd, options)?;
            set_nonblock(fd)?;
            Ok((info, fd))
        })
        .await?;

        let file = AsyncFd::new(unsafe { File::from_raw_fd(fd) })?;
        let info = Arc::new(info);

        Ok(Lines {
            dir: PhantomData,
            info,
            file,
        })
    }
}
