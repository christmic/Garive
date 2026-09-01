//! Generated Rust bindings for admitted public wire contracts.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod process_frame;

pub use process_frame::{
    decode_guest_request_frame, decode_guest_response_frame, decode_host_request_frame,
    decode_host_response_frame, encode_process_frame, ProcessFrameError,
    PROCESS_FRAME_MAX_PAYLOAD_BYTES,
};

/// Experimental common Garive protocol package.
pub mod garive {
    /// Version-one common wire values generated from the admitted Proto SSOT.
    pub mod v1 {
        include!(concat!(env!("OUT_DIR"), "/garive.v1.rs"));
    }
}

/// Java-style package namespace used by the Host API schema.
pub mod com {
    /// Garive product namespace.
    pub mod garive {
        /// Host API namespace.
        pub mod host {
            /// Stable Host API v1 generated bindings.
            pub mod v1 {
                include!(concat!(env!("OUT_DIR"), "/com.garive.host.v1.rs"));
            }
        }

        /// Native process-isolation wire namespace.
        pub mod process {
            /// Version-one process-isolation messages generated from the Proto SSOT.
            pub mod v1 {
                include!(concat!(env!("OUT_DIR"), "/com.garive.process.v1.rs"));
            }
        }
    }
}
