use wasm_bindgen::prelude::*;
use gloo_utils::format::JsValueSerdeExt;

#[wasm_bindgen]
pub struct ROM {
    bytes: Vec<u8>
}

impl ROM {
    pub fn new(vec: Vec<u8>) -> ROM {
        ROM {
            bytes: vec
        }
    }

    pub fn get_rom(&self) -> Vec<u8> {
        self.bytes.clone()
    }
}

#[wasm_bindgen]
impl ROM {
    #[wasm_bindgen(constructor)]
    pub fn from_js(array: Box<[JsValue]>) -> ROM {
        let mut bytes: Vec<u8> = Vec::new();
        for byte in array.iter() {
            if let Ok(val) = byte.into_serde() {
                bytes.push(val);
            }
        }
        ROM {
            bytes
        }
    }
}
