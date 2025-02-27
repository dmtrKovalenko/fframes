#[cfg(target_arch = "wasm32")]
pub mod log {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = console)]
        pub fn console_log(s: &str);
    }

    #[macro_export]
    macro_rules! log {
    ($($t:tt)*) => {
        $crate::console_log(&format_args!($($t)*).to_string())
    }
}
}

#[allow(clippy::module_inception)]
#[cfg(not(target_arch = "wasm32"))]
pub mod log {
    #[macro_export]
    macro_rules! log {
    ( $( $t:tt )* ) => {
      println!( $( $t )* );
    }
    }
}
