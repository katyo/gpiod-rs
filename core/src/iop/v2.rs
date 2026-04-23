use crate::{
    raw::v2::*, utils::*, Active, AsValuesMut, Bias, Direction, Drive, Edge, EdgeDetect, Event,
    LineId, LineInfo, LineMap, Result, Time, Values,
};

/// Raw event ro read from fd
pub type RawEvent = GpioLineEvent;

impl GpioLineInfo {
    pub fn as_info(&self) -> Result<LineInfo> {
        let direction = if is_set(self.flags, GPIO_LINE_FLAG_OUTPUT) {
            Direction::Output
        } else {
            Direction::Input
        };

        let active = if is_set(self.flags, GPIO_LINE_FLAG_ACTIVE_LOW) {
            Active::Low
        } else {
            Active::High
        };

        let edge = match (
            is_set(self.flags, GPIO_LINE_FLAG_EDGE_RISING),
            is_set(self.flags, GPIO_LINE_FLAG_EDGE_FALLING),
        ) {
            (true, false) => EdgeDetect::Rising,
            (false, true) => EdgeDetect::Falling,
            (true, true) => EdgeDetect::Both,
            _ => EdgeDetect::Disable,
        };

        let used = is_set(self.flags, GPIO_LINE_FLAG_USED);

        let bias = match (
            is_set(self.flags, GPIO_LINE_FLAG_BIAS_PULL_UP),
            is_set(self.flags, GPIO_LINE_FLAG_BIAS_PULL_DOWN),
        ) {
            (true, false) => Bias::PullUp,
            (false, true) => Bias::PullDown,
            _ => Bias::Disable,
        };

        let drive = match (
            is_set(self.flags, GPIO_LINE_FLAG_OPEN_DRAIN),
            is_set(self.flags, GPIO_LINE_FLAG_OPEN_SOURCE),
        ) {
            (true, false) => Drive::OpenDrain,
            (false, true) => Drive::OpenSource,
            _ => Drive::PushPull,
        };
        let name = safe_get_str(&self.name)?.into();
        let consumer = safe_get_str(&self.consumer)?.into();

        Ok(LineInfo {
            direction,
            active,
            edge,
            used,
            bias,
            drive,
            name,
            consumer,
        })
    }
}

impl AsMut<GpioLineValues> for Values {
    fn as_mut(&mut self) -> &mut GpioLineValues {
        // it's safe because memory layout is same
        unsafe { &mut *(self as *mut _ as *mut _) }
    }
}

impl GpioLineRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        lines: &[LineId],
        direction: Direction,
        active: Active,
        edge: Option<EdgeDetect>,
        bias: Option<Bias>,
        debounce_period: Option<Time>,
        drive: Option<Drive>,
        values: Option<Values>,
        consumer: &str,
    ) -> Result<Self> {
        let mut request = GpioLineRequest::default();

        check_len(lines, &request.offsets)?;

        request.num_lines = lines.len() as _;

        request.offsets[..lines.len()].copy_from_slice(lines);

        let config = &mut request.config;

        config.flags |= match direction {
            Direction::Input => GPIO_LINE_FLAG_INPUT,
            // Mixing input and output flags is not allowed
            // see https://github.com/torvalds/linux/blob/v5.18/drivers/gpio/gpiolib-cdev.c#L895-L901
            Direction::Output => GPIO_LINE_FLAG_OUTPUT,
        };

        if matches!(active, Active::Low) {
            config.flags |= GPIO_LINE_FLAG_ACTIVE_LOW;
        }

        if matches!(direction, Direction::Input) {
            // Set edge flags is valid only for input
            // see https://github.com/torvalds/linux/blob/v5.18/drivers/gpio/gpiolib-cdev.c#L903-L906
            if let Some(edge) = edge {
                match edge {
                    EdgeDetect::Rising => config.flags |= GPIO_LINE_FLAG_EDGE_RISING,
                    EdgeDetect::Falling => config.flags |= GPIO_LINE_FLAG_EDGE_FALLING,
                    EdgeDetect::Both => config.flags |= GPIO_LINE_FLAG_EDGE_BOTH,
                    _ => {}
                }
            }
        }

        if let Some(bias) = bias {
            config.flags |= match bias {
                Bias::PullUp => GPIO_LINE_FLAG_BIAS_PULL_UP,
                Bias::PullDown => GPIO_LINE_FLAG_BIAS_PULL_DOWN,
                Bias::Disable => GPIO_LINE_FLAG_BIAS_DISABLED,
            }
        }

        if matches!(direction, Direction::Input) {
            if let Some(debounce_period) = debounce_period {
                let debounce_period_us = debounce_period.as_micros().try_into().map_err(|_| {
                    invalid_input("Debounce period is larger than GPIO chardev supports")
                })?;

                config.num_attrs = 1;
                let attr = &mut config.attrs[0];
                attr.attr.id = GPIO_LINE_ATTR_ID_DEBOUNCE;
                attr.mask = if lines.len() == u64::BITS as usize {
                    u64::MAX
                } else {
                    (1u64 << lines.len()) - 1
                };
                attr.attr.val.debounce_period_us = debounce_period_us;
            }
        }

        if matches!(direction, Direction::Output) {
            // Set drive flags is valid only for output
            // see https://github.com/torvalds/linux/blob/v5.18/drivers/gpio/gpiolib-cdev.c#L917-L920
            if let Some(drive) = drive {
                match drive {
                    Drive::OpenDrain => config.flags |= GPIO_LINE_FLAG_OPEN_DRAIN,
                    Drive::OpenSource => config.flags |= GPIO_LINE_FLAG_OPEN_SOURCE,
                    _ => (),
                }
            }

            if let Some(mut values) = values {
                values.truncate(lines.len() as _);

                config.num_attrs = 1;
                let attr = &mut config.attrs[0];
                attr.attr.id = GPIO_LINE_ATTR_ID_OUTPUT_VALUES;
                attr.mask = values.mask;
                attr.attr.val.values = values.bits;
            }
        }

        safe_set_str(&mut request.consumer, consumer)?;

        Ok(request)
    }
}

impl GpioLineEvent {
    pub fn as_event(&self, line_map: &LineMap) -> Result<Event> {
        let line = line_map.get(self.offset)?;

        let edge = match self.id {
            GPIO_LINE_EVENT_RISING_EDGE => Edge::Rising,
            GPIO_LINE_EVENT_FALLING_EDGE => Edge::Falling,
            _ => return Err(invalid_data("Unknown edge")),
        };

        let time = time_from_nanos(self.timestamp_ns);

        Ok(Event { line, edge, time })
    }
}
