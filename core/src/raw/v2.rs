use super::{GPIO_MAGIC, GPIO_MAX_NAME_SIZE};

pub const GPIO_LINES_MAX: usize = 64;
pub const GPIO_LINE_NUM_ATTRS_MAX: usize = 10;

// Flags for line
pub const GPIO_LINE_FLAG_USED: u64 = 1 << 0;
pub const GPIO_LINE_FLAG_ACTIVE_LOW: u64 = 1 << 1;
pub const GPIO_LINE_FLAG_INPUT: u64 = 1 << 2;
pub const GPIO_LINE_FLAG_OUTPUT: u64 = 1 << 3;
pub const GPIO_LINE_FLAG_EDGE_RISING: u64 = 1 << 4;
pub const GPIO_LINE_FLAG_EDGE_FALLING: u64 = 1 << 5;
pub const GPIO_LINE_FLAG_EDGE_BOTH: u64 = GPIO_LINE_FLAG_EDGE_RISING | GPIO_LINE_FLAG_EDGE_FALLING;
pub const GPIO_LINE_FLAG_OPEN_DRAIN: u64 = 1 << 6;
pub const GPIO_LINE_FLAG_OPEN_SOURCE: u64 = 1 << 7;
pub const GPIO_LINE_FLAG_BIAS_PULL_UP: u64 = 1 << 8;
pub const GPIO_LINE_FLAG_BIAS_PULL_DOWN: u64 = 1 << 9;
pub const GPIO_LINE_FLAG_BIAS_DISABLED: u64 = 1 << 10;
//pub const GPIO_LINE_FLAG_EVENT_CLOCK_REALTIME: u64 = 1 << 11;
//pub const GPIO_LINE_FLAG_EVENT_CLOCK_HTE: u64 = 1 << 12;

// Line attr ids
//pub const GPIO_LINE_ATTR_ID_FLAGS: u32 = 1;
pub const GPIO_LINE_ATTR_ID_OUTPUT_VALUES: u32 = 2;
pub const GPIO_LINE_ATTR_ID_DEBOUNCE: u32 = 3;

// Line changed reason
//pub const GPIO_LINE_CHANGED_REQUESTED: u32 = 1;
//pub const GPIO_LINE_CHANGED_RELEASED: u32 = 2;
//pub const GPIO_LINE_CHANGED_CONFIG: u32 = 3;

// Line event edge
pub const GPIO_LINE_EVENT_RISING_EDGE: u32 = 1;
pub const GPIO_LINE_EVENT_FALLING_EDGE: u32 = 2;

#[derive(Clone, Copy)]
#[repr(C)]
pub union GpioLineAttrVal {
    pub flags: u64,
    pub values: u64,
    pub debounce_period_us: u32,
}

impl Default for GpioLineAttrVal {
    fn default() -> Self {
        Self {
            values: Default::default(),
        }
    }
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct GpioLineAttr {
    pub id: u32,
    padding: u32,
    pub val: GpioLineAttrVal,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct GpioLineConfigAttr {
    pub attr: GpioLineAttr,
    pub mask: u64,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct GpioLineConfig {
    pub flags: u64,
    pub num_attrs: u32,
    padding: [u32; 5],
    pub attrs: [GpioLineConfigAttr; GPIO_LINE_NUM_ATTRS_MAX],
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct GpioLineRequest {
    pub offsets: [u32; GPIO_LINES_MAX],
    pub consumer: [u8; GPIO_MAX_NAME_SIZE],
    pub config: GpioLineConfig,
    pub num_lines: u32,
    pub event_buffer_size: u32,
    padding: [u32; 5],
    pub fd: i32,
}

impl Default for GpioLineRequest {
    fn default() -> Self {
        Self {
            offsets: [Default::default(); GPIO_LINES_MAX],
            consumer: Default::default(),
            config: Default::default(),
            num_lines: Default::default(),
            event_buffer_size: Default::default(),
            padding: Default::default(),
            fd: Default::default(),
        }
    }
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct GpioLineInfo {
    pub name: [u8; GPIO_MAX_NAME_SIZE],
    pub consumer: [u8; GPIO_MAX_NAME_SIZE],
    pub offset: u32,
    pub num_attrs: u32,
    pub flags: u64,
    pub attrs: [GpioLineAttr; GPIO_LINE_NUM_ATTRS_MAX],
    padding: [u32; 4],
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct GpioLineInfoChanged {
    pub info: GpioLineInfo,
    pub timestamp_ns: u64,
    pub event_type: u32,
    padding: [u32; 5],
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct GpioLineEvent {
    pub timestamp_ns: u64,
    pub id: u32,
    pub offset: u32,
    pub seqno: u32,
    pub line_seqno: u32,
    padding: [u32; 6],
}

impl AsMut<[u8; core::mem::size_of::<GpioLineEvent>()]> for GpioLineEvent {
    fn as_mut(&mut self) -> &mut [u8; core::mem::size_of::<GpioLineEvent>()] {
        unsafe { core::mem::transmute(self) }
    }
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct GpioLineValues {
    pub bits: u64,
    pub mask: u64,
}

nix::ioctl_readwrite!(gpio_get_line_info, GPIO_MAGIC, 0x05, GpioLineInfo);
nix::ioctl_readwrite!(gpio_get_line_info_watch, GPIO_MAGIC, 0x06, GpioLineInfo);
nix::ioctl_readwrite!(gpio_get_line, GPIO_MAGIC, 0x07, GpioLineRequest);
nix::ioctl_readwrite!(gpio_line_set_config, GPIO_MAGIC, 0x0d, GpioLineConfig);
nix::ioctl_readwrite!(gpio_line_get_values, GPIO_MAGIC, 0x0e, GpioLineValues);
nix::ioctl_readwrite!(gpio_line_set_values, GPIO_MAGIC, 0x0f, GpioLineValues);

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn sizes() {
        use core::mem::size_of;

        assert_eq!(size_of::<GpioLineAttr>(), 16);
        assert_eq!(size_of::<GpioLineConfigAttr>(), 24);
        assert_eq!(size_of::<GpioLineConfig>(), 272);
        assert_eq!(size_of::<GpioLineRequest>(), 592);
        assert_eq!(size_of::<GpioLineInfo>(), 256);
        assert_eq!(size_of::<GpioLineInfoChanged>(), 288);
        assert_eq!(size_of::<GpioLineEvent>(), 48);
        assert_eq!(size_of::<GpioLineValues>(), 16);
    }
}
