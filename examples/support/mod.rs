use std::error::Error;

use drywet::{Context, DeviceSink, DeviceSinkError};

pub fn device_context() -> Result<Context<DeviceSink>, DeviceSinkError> {
    let sink = DeviceSink::new()?;
    Ok(Context::with(sink.sample_rate(), sink.channels(), sink))
}

pub fn play(ctx: &Context<DeviceSink>, duration: &str) -> Result<(), Box<dyn Error>> {
    ctx.render(duration)?;
    ctx.sink_mut().play()?;
    ctx.sink().wait_until_end()?;
    Ok(())
}
