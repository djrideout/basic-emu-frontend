mod audio;
mod display;
pub mod keymap;

use crate::audio::AudioPlayer;
use crate::display::Display;
use crate::keymap::Keymap;
use clap::ValueEnum;
use std::{future::Future, sync::{Arc, Mutex}};
pub use winit::event::VirtualKeyCode;

#[derive(PartialEq, Clone, Copy, Default, ValueEnum, Debug)]
pub enum SyncModes {
    // One rendering frame is one execution frame. Audio is disabled.
    VSync,
    // Execution occurs when the audio device needs more samples.
    // Higher buffer size means smoother audio but rougher frame rate and vice versa.
    #[default]
    AudioCallback
}

pub trait Core: Send + 'static {
    fn get_width(&self) -> usize;
    fn get_height(&self) -> usize;
    fn get_sample_queue_length(&self) -> usize;
    fn draw(&self, frame: &mut [u8]);
    fn set_seconds_per_output_sample(&mut self, value: f32);
    fn set_num_output_channels(&mut self, value: usize);
    fn press_key(&mut self, key_index: usize);
    fn release_key(&mut self, key_index: usize);
    fn run_inst(&mut self);
    fn run_frame(&mut self);
    fn get_sample(&mut self) -> f32;
}

pub struct Frontend {
    display: display::Display,
    audio_player: audio::AudioPlayer
}

impl Frontend {
    pub fn new(core: impl Core, keymap: Keymap, sync_mode: SyncModes) -> Frontend {
        // Create Arcs to share the core between the audio and rendering threads
        let arc_parent = Arc::new(Mutex::new(core));
        let arc_child = arc_parent.clone();

        let get_sample = move || {
            // Lock the mutex while generating samples in the audio thread
            let mut core = arc_child.lock().unwrap();
            match sync_mode {
                SyncModes::AudioCallback => {
                    // Run instructions until a new sample is ready and return that
                    while core.get_sample_queue_length() == 0 {
                        core.run_inst();
                    }
                    core.get_sample()
                },
                SyncModes::VSync => {
                    // Audio is disabled with vsync, so just dump the samples and return 0
                    while core.get_sample_queue_length() > 0 {
                        core.get_sample();
                    }
                    0.0
                }
            }
        };
        let audio_player = AudioPlayer::new(get_sample);

        let arc_temp = arc_parent.clone();
        let mut core_temp = arc_temp.lock().unwrap();
        core_temp.set_seconds_per_output_sample(1.0 / audio_player.get_sample_rate() as f32);
        core_temp.set_num_output_channels(audio_player.get_num_channels());
        drop(core_temp);

        let display = Display::new(arc_parent, keymap, sync_mode);

        Frontend {
            display,
            audio_player
        }
    }

    pub async fn start(&self) {
        self.audio_player.run();
        self.display.run().await
    }
}

pub fn block_on<F: Future<Output = ()> + 'static>(fut: F) {
    #[cfg(target_arch = "wasm32")]
    {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init_with_level(log::Level::Trace).expect("error initializing logger");
        wasm_bindgen_futures::spawn_local(fut);
    }
    #[cfg(not(target_arch = "wasm32"))]
    pollster::block_on(fut);
}
