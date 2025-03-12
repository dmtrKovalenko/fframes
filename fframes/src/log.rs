#[cfg(target_arch = "wasm32")]
pub mod log_impl {
    use wasm_bindgen::prelude::*;

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen]
    unsafe extern "C" {
        #[doc(hidden)]
        #[wasm_bindgen(js_namespace = console, js_name = log)]
        pub fn _console_log(s: &str);
    }

    #[macro_export]
    macro_rules! log {
        ($($tt:tt)*)  => {
            $crate::log::log_impl::_console_log(&format!($($tt)*))
        };
    }
}

#[allow(clippy::module_inception)]
#[cfg(not(target_arch = "wasm32"))]
pub mod log_impl {
    #[macro_export]
    macro_rules! log {
    ( $( $t:tt )* ) => {
      println!( $( $t )* );
    }
    }
}
