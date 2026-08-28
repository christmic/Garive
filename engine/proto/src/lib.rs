//! Generated Rust bindings for admitted public wire contracts.

#![forbid(unsafe_code)]

pub mod garive {
    pub mod v1 {
        include!(concat!(env!("OUT_DIR"), "/garive.v1.rs"));
    }
}

pub mod com {
    pub mod garive {
        pub mod host {
            pub mod v1 {
                include!(concat!(env!("OUT_DIR"), "/com.garive.host.v1.rs"));
            }
        }
    }
}
