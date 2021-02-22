use std::{convert::TryInto, io, path::Path, time::Duration};

use v4l::control::{Control, MenuItem as ControlMenuItem, Type as ControlType};
use v4l::video::Capture;
use v4l::Device as CaptureDevice;
use v4l::Format as CaptureFormat;
use v4l::FourCC as FourCC_;

use crate::control;
use crate::format::PixelFormat;
use crate::hal::v4l2::stream::Handle as StreamHandle;
use crate::stream::{
    Descriptor as StreamDescriptor, Descriptors as StreamDescriptors, FrameStream,
};
use crate::traits::Device;

pub struct Handle {
    inner: CaptureDevice,
}

impl Handle {
    pub fn new(index: usize) -> io::Result<Self> {
        let dev = Handle {
            inner: CaptureDevice::new(index)?,
        };
        Ok(dev)
    }

    pub fn with_path<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let dev = Handle {
            inner: CaptureDevice::with_path(path)?,
        };
        Ok(dev)
    }

    pub fn inner(&self) -> &CaptureDevice {
        &self.inner
    }
}

impl<'a> Device<'a> for Handle {
    fn query_streams(&self) -> io::Result<StreamDescriptors> {
        let mut streams = Vec::new();
        let plat_formats = self.inner.enum_formats()?;

        for format in plat_formats {
            for framesize in self.inner.enum_framesizes(format.fourcc)? {
                // TODO: consider stepwise formats
                if let v4l::framesize::FrameSizeEnum::Discrete(size) = framesize.size {
                    for frameinterval in
                        self.inner
                            .enum_frameintervals(format.fourcc, size.width, size.height)?
                    {
                        // TODO: consider stepwise intervals
                        if let v4l::frameinterval::FrameIntervalEnum::Discrete(fraction) =
                            frameinterval.interval
                        {
                            streams.push(StreamDescriptor {
                                width: size.width,
                                height: size.height,
                                pixfmt: PixelFormat::from(&format.fourcc.repr),
                                interval: Duration::from_secs_f64(
                                    fraction.numerator as f64 / fraction.denominator as f64,
                                ),
                            });
                        }
                    }
                }
            }
        }

        Ok(StreamDescriptors { streams })
    }

    fn query_controls(&self) -> io::Result<Vec<control::Control>> {
        let mut controls = Vec::new();
        let plat_controls = self.inner.query_controls()?;

        for control in plat_controls {
            // The v4l docs say applications should ignore permanently disabled controls.
            if control.flags & v4l::control::Flags::DISABLED == v4l::control::Flags::DISABLED {
                continue;
            }

            let mut repr = control::Representation::Unknown;
            match control.typ {
                ControlType::Integer | ControlType::Integer64 => {
                    repr = control::Representation::Integer {
                        range: (control.minimum as i64, control.maximum as i64),
                        step: control.step as u64,
                        default: control.default as i64,
                    };
                }
                ControlType::Boolean => {
                    repr = control::Representation::Boolean;
                }
                ControlType::Menu => {
                    let mut items = Vec::new();
                    if let Some(plat_items) = control.items {
                        for plat_item in plat_items {
                            match plat_item.1 {
                                ControlMenuItem::Name(name) => {
                                    items.push(control::MenuItem::String(name));
                                }
                                ControlMenuItem::Value(value) => {
                                    items.push(control::MenuItem::Integer(value));
                                }
                            }
                        }
                    }
                    repr = control::Representation::Menu(items);
                }
                ControlType::Button => {
                    repr = control::Representation::Button;
                }
                ControlType::String => {
                    repr = control::Representation::String;
                }
                ControlType::Bitmask => {
                    repr = control::Representation::Bitmask;
                }
                _ => {}
            }

            let mut flags = control::Flags::NONE;
            if control.flags & v4l::control::Flags::READ_ONLY == v4l::control::Flags::READ_ONLY {
                flags |= control::Flags::READ_ONLY;
            }
            if control.flags & v4l::control::Flags::WRITE_ONLY == v4l::control::Flags::WRITE_ONLY {
                flags |= control::Flags::WRITE_ONLY;
            }
            if control.flags & v4l::control::Flags::GRABBED == v4l::control::Flags::GRABBED {
                flags |= control::Flags::GRABBED;
            }
            if control.flags & v4l::control::Flags::INACTIVE == v4l::control::Flags::INACTIVE {
                flags |= control::Flags::INACTIVE;
            }

            controls.push(control::Control {
                id: control.id,
                name: control.name,
                repr,
                flags,
            })
        }

        Ok(controls)
    }

    fn control(&self, id: u32) -> io::Result<control::Value> {
        let ctrl = self.inner.control(id)?;
        match ctrl {
            Control::Value(val) => Ok(control::Value::Integer(val as i64)),
            _ => Err(io::Error::new(
                io::ErrorKind::Other,
                "control type cannot be mapped",
            )),
        }
    }

    fn set_control(&mut self, id: u32, val: &control::Value) -> io::Result<()> {
        match val {
            control::Value::Integer(val) => {
                let ctrl = Control::Value(*val as i32);
                self.inner.set_control(id, ctrl)?;
            }
            control::Value::Boolean(val) => {
                let ctrl = Control::Value(*val as i32);
                self.inner.set_control(id, ctrl)?;
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "control type cannot be mapped",
                ))
            }
        }

        Ok(())
    }

    fn preferred_stream(
        &self,
        f: &dyn Fn(StreamDescriptor, StreamDescriptor) -> StreamDescriptor,
    ) -> io::Result<StreamDescriptor> {
        let mut preferred = None;
        let streams = self.query_streams()?.streams;
        if streams.len() == 1 {
            preferred = Some(streams[0].clone());
        } else if streams.len() > 1 {
            for i in 0..streams.len() - 2 {
                preferred = Some(f(streams[i].clone(), streams[i + 1].clone()));
            }
        }

        match preferred {
            Some(desc) => Ok(desc),
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "no stream desciptors available",
                ))
            }
        }
    }

    fn start_stream(&self, desc: &StreamDescriptor) -> io::Result<FrameStream<'a>> {
        let fourcc = if let Ok(fourcc) = desc.pixfmt.clone().try_into() {
            fourcc
        } else {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "failed to map pixelformat to fourcc",
            ));
        };
        // configure frame format
        let format = CaptureFormat::new(desc.width, desc.height, FourCC_::new(&fourcc));
        self.inner.set_format(&format)?;

        // configure frame timing
        let fps = (1.0 / desc.interval.as_secs_f32()) as u32;
        let mut params = self.inner.params()?;
        params.interval = v4l::Fraction::new(1, fps);
        self.inner.set_params(&params)?;

        let stream = StreamHandle::new(self)?;
        Ok(FrameStream::new(Box::new(stream)))
    }
}
