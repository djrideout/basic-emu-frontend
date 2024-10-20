use crate::{Core, SyncModes};
use crate::keymap::Keymap;
use wasm_bindgen::prelude::*;
use error_iter::ErrorIter as _;
use log::error;
use std::sync::{Arc, Mutex};
use pixels::{Pixels, SurfaceTexture};
use std::rc::Rc;
use std::cell::RefCell;
use winit::dpi::LogicalSize;
use winit::event::{Event, VirtualKeyCode};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;
use winit_input_helper::WinitInputHelper;
use gloo_utils::format::JsValueSerdeExt;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = wasm_imports)]
    fn on_key_pressed(i: usize);

    #[wasm_bindgen(js_namespace = wasm_imports)]
    fn on_key_released(i: usize);
}

thread_local! {
    pub static KEY_OVERRIDES: RefCell<Vec<VirtualKeyCode>> = RefCell::new(vec![]);
}

// Programmatically press key_code.
// This overrides the normal input handling in winit's event loop.
#[wasm_bindgen]
pub fn press_key(key_code: JsValue) {
    if let Ok(virtual_key) = key_code.into_serde::<VirtualKeyCode>() {
        KEY_OVERRIDES.with(|vec| {
            vec.borrow_mut().push(virtual_key);
        });
    }
}

// Programmatically release key_code.
// This overrides the normal input handling in winit's event loop.
#[wasm_bindgen]
pub fn release_key(key_code: JsValue) {
    if let Ok(virtual_key) = key_code.into_serde::<VirtualKeyCode>() {
        KEY_OVERRIDES.with(|vec| {
            let mut vec = vec.borrow_mut();
            if let Some(index) = vec.iter().position(|value| *value == virtual_key) {
                vec.swap_remove(index);
            }
        });
    }
}

pub struct Display {
    core: Arc<Mutex<dyn Core>>,
    width: usize,
    height: usize,
    keymap: Keymap,
    sync_mode: SyncModes
}

impl Display {
    pub fn new(core: Arc<Mutex<impl Core>>, keymap: Keymap, sync_mode: SyncModes) -> Display {
        let core_temp = core.lock().unwrap();
        let width = core_temp.get_width();
        let height = core_temp.get_height();
        drop(core_temp);
        Display {
            core,
            width,
            height,
            keymap,
            sync_mode
        }
    }

    pub async fn run(&self) {
        // Set up graphics buffer and window
        let event_loop = EventLoop::new();
        let mut input = WinitInputHelper::new();
        let window = {
            let size = LogicalSize::new(self.width as f64, self.height as f64);
            WindowBuilder::new()
                .with_title("chippy")
                .with_inner_size(size.to_physical::<f64>(5.0))
                .with_min_inner_size(size)
                .build(&event_loop)
                .expect("WindowBuilder error")
        };

        let window = Rc::new(window);

        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            use winit::platform::web::WindowExtWebSys;

            // Retrieve current width and height dimensions of browser client window
            let get_window_size = || {
                let client_window = web_sys::window().unwrap();
                LogicalSize::new(
                    client_window.inner_width().unwrap().as_f64().unwrap(),
                    client_window.inner_height().unwrap().as_f64().unwrap(),
                )
            };

            let window = Rc::clone(&window);

            // Initialize winit window with current dimensions of browser client
            window.set_inner_size(get_window_size());

            let client_window = web_sys::window().unwrap();

            // Attach winit canvas to body element
            let mut appended_to_body = false;
            web_sys::window()
                .and_then(|win| win.document())
                .and_then(|doc| doc.body())
                .and_then(|body| {
                    match body.query_selector("#emulator") {
                        Ok(Some(el)) => Some(el),
                        _ => {
                            appended_to_body = true;
                            Some(body.into())
                        }
                    }
                })
                .and_then(|container| {
                    container.append_child(&web_sys::Element::from(window.canvas()))
                        .ok()
                })
                .expect("couldn't append canvas to `#emulator` element or body");

            if appended_to_body {
                // If appended to the body, listen for resize event on browser client. Adjust winit window dimensions
                // on event trigger
                let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move |_e: web_sys::Event| {
                    let size = get_window_size();
                    window.set_inner_size(size)
                }) as Box<dyn FnMut(_)>);
                client_window
                    .add_event_listener_with_callback("resize", closure.as_ref().unchecked_ref())
                    .unwrap();
                closure.forget();
            } else {
                // If appended to the `#emulator` container element, just set the width/height to the
                // width/height of the core. The page styles will be expected to override the inline
                // width/height styles of the canvas.
                window.set_inner_size(LogicalSize::new(
                    self.width as f64,
                    self.height as f64
                ));
            }
        }

        let mut pixels = {
            let window_size = window.inner_size();
            let surface_texture = SurfaceTexture::new(window_size.width, window_size.height, window.as_ref());
            Pixels::new_async(self.width as u32, self.height as u32, surface_texture).await.expect("Pixels error")
        };

        let core = self.core.clone();
        let keymap = self.keymap.get_keys();
        let sync_mode = self.sync_mode;

        event_loop.run(move |event, _, control_flow| {
            // Draw the current frame
            if let Event::RedrawRequested(_) = event {
                let core = core.lock().unwrap();
                core.draw(pixels.frame_mut());
                drop(core);
                if let Err(err) = pixels.render() {
                    log_error("pixels.render", err);
                    *control_flow = ControlFlow::Exit;
                    return;
                }
            }

            // Handle input events
            if input.update(&event) {
                // Close events
                if input.key_pressed(VirtualKeyCode::Escape) || input.close_requested() {
                    *control_flow = ControlFlow::Exit;
                    return;
                }

                // Resize the window
                if let Some(size) = input.window_resized() {
                    if let Err(err) = pixels.resize_surface(size.width, size.height) {
                        log_error("pixels.resize_surface", err);
                        *control_flow = ControlFlow::Exit;
                        return;
                    }
                }

                KEY_OVERRIDES.with(|vec| {
                    let overrides = vec.borrow();
                    let mut core = core.lock().unwrap();
                    // Handle key presses
                    for i in 0 .. keymap.len() {
                        let _prev_key_pressed = core.get_key_pressed(i);
                        let key_override = overrides.iter().position(|value| *value == keymap[i]);
                        if key_override.is_some() || input.key_pressed(keymap[i]) || input.key_held(keymap[i]) {
                            core.press_key(i);
                        } else {
                            core.release_key(i);
                        }
                        #[cfg(target_arch = "wasm32")]
                        if core.get_key_pressed(i) && !_prev_key_pressed {
                            on_key_pressed(i);
                        }
                        #[cfg(target_arch = "wasm32")]
                        if !core.get_key_pressed(i) && _prev_key_pressed {
                            on_key_released(i);
                        }
                    }
                    if sync_mode == SyncModes::VSync {
                        core.run_frame();
                    }
                });

                window.request_redraw();
            }
        })
    }
}

fn log_error<E: std::error::Error + 'static>(method_name: &str, err: E) {
    error!("{method_name}() failed: {err}");
    for source in err.sources().skip(1) {
        error!("  Caused by: {source}");
    }
}
