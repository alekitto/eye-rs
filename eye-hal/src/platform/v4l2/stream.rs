use v4l::buffer::Type as BufType;
use v4l::io::mmap::Stream as MmapStream;
use v4l::io::traits::CaptureStream;

use crate::buffer::Buffer;
use crate::error::Result;
use crate::platform::v4l2::device::Handle as DeviceHandle;
use crate::traits::Stream;

pub struct Handle<'a> {
    stream: MmapStream<'a>,
}

impl<'a> Handle<'a> {
    pub fn new(dev: &DeviceHandle) -> Result<Self> {
        let stream = MmapStream::new(dev.inner(), BufType::VideoCapture)?;
        Ok(Handle { stream })
    }

    pub fn with_buffers(dev: &DeviceHandle, buf_count: u32) -> Result<Self> {
        let stream = MmapStream::with_buffers(dev.inner(), BufType::VideoCapture, buf_count)?;
        Ok(Handle { stream })
    }
}

impl<'a, 'b> Stream<'b> for Handle<'a> {
    type Item = Result<Buffer<'b>>;

    fn next(&'b mut self) -> Option<Self::Item> {
        match CaptureStream::next(&mut self.stream) {
            Err(e) => Some(Err(e.into())),
            Ok(None) => None,
            Ok(Some((buffer, meta))) => {
                let view = &buffer[0..meta.bytesused as usize];

                Some(Ok(Buffer::from(view)))
            }
        }
    }
}
