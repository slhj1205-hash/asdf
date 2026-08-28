use std::time::Duration;

use gstreamer as gst;
use gstreamer::prelude::*;

use crate::player::{AudioBackend, BackendError, BackendEvent};

pub struct GstBackend {
    playbin: gst::Element,
    bus: gst::Bus,
}

impl GstBackend {
    pub fn new() -> Result<GstBackend, BackendError> {
        gst::init()?;

        let playbin = gst::ElementFactory::make("playbin")
            .build()
            .map_err(|_| "required gstreamer element 'playbin' is not installed")?;

        let bus = playbin.bus().ok_or("playbin has no bus")?;

        Ok(GstBackend { playbin, bus })
    }
}

impl AudioBackend for GstBackend {
    fn describe(&self) -> &'static str {
        "gstreamer (real audio)"
    }

    fn play_uri(&mut self, uri: &str) -> Result<(), BackendError> {
        self.playbin.set_state(gst::State::Null)?;
        self.playbin.set_property("uri", uri);
        self.playbin.set_state(gst::State::Playing)?;
        Ok(())
    }

    fn pause(&mut self) -> Result<(), BackendError> {
        self.playbin.set_state(gst::State::Paused)?;
        Ok(())
    }

    fn resume(&mut self) -> Result<(), BackendError> {
        self.playbin.set_state(gst::State::Playing)?;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), BackendError> {
        self.playbin.set_state(gst::State::Null)?;
        Ok(())
    }

    fn set_volume(&mut self, volume: f64) {
        self.playbin.set_property("volume", volume);
    }

    fn volume(&self) -> f64 {
        self.playbin.property("volume")
    }

    fn position(&self) -> Option<Duration> {
        self.playbin
            .query_position::<gst::ClockTime>()
            .map(|t| Duration::from_nanos(t.nseconds()))
    }

    fn duration(&self) -> Option<Duration> {
        self.playbin
            .query_duration::<gst::ClockTime>()
            .map(|t| Duration::from_nanos(t.nseconds()))
    }

    fn seek(&mut self, position: Duration) -> Result<(), BackendError> {
        let clock_time = gst::ClockTime::from_nseconds(position.as_nanos() as u64);
        self.playbin
            .seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, clock_time)?;
        Ok(())
    }

    fn poll_events(&mut self) -> Vec<BackendEvent> {
        let mut events = Vec::new();
        while let Some(msg) = self.bus.pop() {
            match msg.view() {
                gst::MessageView::Eos(_) => events.push(BackendEvent::EndOfStream),
                gst::MessageView::Error(err) => {
                    let text = format!("{} ({:?})", err.error(), err.debug());
                    events.push(BackendEvent::Error(text));
                }
                _ => {}
            }
        }
        events
    }
}
