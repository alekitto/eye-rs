use std::time;

use crate::format::PixelFormat;

#[derive(Clone, Debug)]
/// Image stream description
pub struct Descriptor {
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// PixelFormat
    pub pixfmt: PixelFormat,
    /// Frame timing as duration
    pub interval: time::Duration,
}

#[derive(Clone, Debug)]
/// Stream settings, needed on stream open operation.
pub struct DeviceStreamSettings<'a> {
    pub desc: &'a Descriptor,
    pub buffers_count: Option<usize>,
}
