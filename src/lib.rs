mod audio;
mod display;
pub mod keymap;

use wasm_bindgen::prelude::*;
use crate::audio::AudioPlayer;
use crate::display::Display;
use crate::keymap::Keymap;
use clap::ValueEnum;
use std::{future::Future, sync::{Arc, Mutex}};
pub use winit::event::VirtualKeyCode;

use std::path::PathBuf;
use std::ffi::{OsStr, OsString};
use std::env;
use std::process::Command;

#[wasm_bindgen]
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
    fn get_key_pressed(&self, key_index: usize) -> bool;
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
    audio_player: audio::AudioPlayer,
    sync_mode: Arc<Mutex<SyncModes>>
}

impl Frontend {
    pub fn new(core: Arc<Mutex<impl Core>>, keymap: Keymap, sync_mode: SyncModes) -> Frontend {
        // Create Arcs to share the core between the audio and rendering threads
        let core_arc_display = core.clone();
        let core_arc_audio = core.clone();
        let sync_arc = Arc::new(Mutex::new(sync_mode));
        let sync_arc_audio = sync_arc.clone();

        let get_sample = move || {
            // Lock the mutex while generating samples in the audio thread
            let mut core = core_arc_audio.lock().unwrap();
            match *sync_arc_audio.lock().unwrap() {
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

        let arc_frontend = core_arc_display.clone();
        let mut core_temp = arc_frontend.lock().unwrap();
        core_temp.set_seconds_per_output_sample(1.0 / audio_player.get_sample_rate() as f32);
        core_temp.set_num_output_channels(audio_player.get_num_channels());
        drop(core_temp);

        let display = Display::new(core_arc_display, keymap, sync_arc.clone());

        Frontend {
            display,
            audio_player,
            sync_mode: sync_arc
        }
    }

    pub async fn start(&self) {
        self.audio_player.start();
        self.display.start().await
    }

    pub fn set_sync_mode(&self, sync_mode: SyncModes) {
        *self.sync_mode.lock().unwrap() = sync_mode;
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

pub fn build_wasm_bindgen(package_name: &OsStr) {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());

    // Build the WASM with Cargo
    let cargo_args: Vec<&OsStr> = vec![
        "build".as_ref(),
        "--release".as_ref(),
        "--package".as_ref(),
        package_name,
        "--target".as_ref(),
        "wasm32-unknown-unknown".as_ref()
    ];
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(&cargo)
        .current_dir(&manifest_dir)
        .args(&cargo_args)
        .status()
        .unwrap();
    if !status.success() {
        println!("Failed due to cargo error");
        return;
    }

    // Generate the JS binds using wasm-bindgen
    let mut binary_name = OsString::from(package_name);
    binary_name.push(OsString::from(".wasm"));
    let wasm_source = manifest_dir.clone()
        .join("..")
        .join("..")
        .join("target")
        .join("wasm32-unknown-unknown")
        .join("release")
        .join(binary_name);
    let bindgen_dest = manifest_dir.clone()
        .join("..")
        .join("..")
        .join("web")
        .join("view")
        .join("wasm");
    let mut bindgen = wasm_bindgen_cli_support::Bindgen::new();
    bindgen
        .typescript(true)
        .web(true)
        .unwrap()
        .omit_default_module_path(false)
        .input_path(&wasm_source)
        .generate(&bindgen_dest)
        .unwrap();
}
