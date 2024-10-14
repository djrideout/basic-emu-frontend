use wasm_bindgen::prelude::*;
use winit::event::VirtualKeyCode;
use gloo_utils::format::JsValueSerdeExt;

#[wasm_bindgen]
pub struct Keymap {
    keys: Vec<VirtualKeyCode>
}

impl Keymap {
    pub fn new(array: &[VirtualKeyCode]) -> Keymap {
        Keymap {
            keys: array.to_vec()
        }
    }

    pub fn get_keys(&self) -> Vec<VirtualKeyCode> {
        self.keys.clone()
    }
}

#[wasm_bindgen]
impl Keymap {
    #[wasm_bindgen(constructor)]
    // The JsValues should be the key values as strings, not numeric values
    // https://docs.rs/winit-gtk/latest/winit/event/enum.VirtualKeyCode.html
    pub fn from_js(array: Box<[JsValue]>) -> Keymap {
        let mut keys: Vec<VirtualKeyCode> = Vec::new();
        for key_code in array.iter() {
            if let Ok(virtual_key) = key_code.into_serde() {
                keys.push(virtual_key);
            }
        }
        Keymap {
            keys
        }
    }
}
