use std::thread::sleep;
use std::time::{Duration, Instant};

use eye_hal::stream::DeviceStreamSettings;
use eye_hal::traits::{Context, Device, Stream};
use eye_hal::{PlatformContext, Result};

fn main() -> Result<()> {
    // Create a context
    let ctx = PlatformContext::default();

    // Query for available devices.
    let devices = ctx.devices()?;

    // First, we need a capture device to read images from. For this example, let's just choose
    // whatever device is first in the list.
    let dev = ctx.open_device(&devices[0].uri)?;

    // Query for available streams and just choose the first one.
    let streams = dev.streams()?;
    let stream_desc = streams[0].clone();
    println!("Stream: {:?}", stream_desc);

    // Since we want to capture images, we need to access the native image stream of the device.
    // The backend will internally select a suitable implementation for the platform stream. On
    // Linux for example, most devices support memory-mapped buffers.
    let mut stream = dev.start_stream(DeviceStreamSettings {
        desc: &stream_desc,
        buffers_count: None,
    })?;

    // Here we create a loop and just capture images as long as the device produces them. Normally,
    // this loop will run forever unless we unplug the camera or exit the program.
    let mut megabytes_ps: f64 = 0.0;
    let mut i = 0;
    loop {
        let t0 = Instant::now();
        let next_frame = stream.next();
        if next_frame.is_none() {
            sleep(Duration::from_millis(1));
            continue;
        }

        let frame = next_frame.unwrap().expect("Failed to capture frame");
        let duration_us = t0.elapsed().as_micros();

        let cur = frame.as_bytes().len() as f64 / 1_048_576.0 * 1_000_000.0 / duration_us as f64;
        if i == 0 {
            megabytes_ps = cur;
        } else {
            // ignore the first measurement
            let prev = megabytes_ps * (i as f64 / (i + 1) as f64);
            let now = cur * (1.0 / (i + 1) as f64);
            megabytes_ps = prev + now;
        }

        i += 1;

        println!(
            "\rFrame: {:.2} ms, MB/s: {:.2}",
            duration_us as f64 / 1000.0,
            megabytes_ps
        );
    }
}
